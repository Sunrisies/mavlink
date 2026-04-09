mod logger;

use anyhow::Result;
use logger::init_logger;
use mavlink::{
    MavConnection, MavHeader, Message,
    ardupilotmega::{MISSION_REQUEST_INT_DATA, MISSION_REQUEST_LIST_DATA, MavMessage},
    error::MessageReadError,
};
use rumqttc::v5::{
    AsyncClient, Event, EventLoop, MqttOptions,
    mqttbytes::{QoS, v5::Packet},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{sync::Arc, thread, time::Duration};
use tokio::sync::mpsc::{self, error::TryRecvError};
use tokio::task;
#[derive(Debug)]
enum MavlinkCommand {
    RequestWaypointList, // 请求航点列表
    RequestWaypoint(u16), // 请求指定序号的航点
                         // 可以添加其他命令类型
}
#[derive(Debug)]
enum MqttCommand {
    GetWaypoints,
}
// 在主函数或顶层定义状态
enum MissionState {
    Idle,
    WaitingCount,
    Downloading {
        expected_count: u16,
        received: Vec<Waypoint>,
    },
}
// 假设 payload 结构如下
#[derive(Debug, Deserialize)]
struct Payload {
    r#type: String,
    // 其他字段...
}

// 定义航点结构体，用于序列化响应
#[derive(Debug, Serialize, Deserialize, Clone)]
struct Waypoint {
    seq: u16,
    lat: f64,
    lon: f64,
    alt: f32,
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logger();
    let conn_str = String::from("udpin:127.0.0.1:2345");
    let topic = String::from("mavlink/incoming");
    let (client, mut eventloop) = setup_mqtt()?;
    // 2. 创建消息通道
    // 从 MAVLink 线程到主循环
    let (mavlink_tx, mut mavlink_rx) = mpsc::channel(256);
    // 从主循环到 MAVLink 线程（用于发送命令）
    let (cmd_tx, cmd_rx) = mpsc::channel::<MavlinkCommand>(256);
    let tx_clone = mavlink_tx.clone();
    thread::spawn(move || {
        log::info!("---------");
        if let Err(e) = mavlink_worker_thread(conn_str, tx_clone, cmd_rx) {
            log::error!("MAVLink 工作线程崩溃: {}", e);
        }
    });
    let (mqtt_cmd_tx, mut mqtt_cmd_rx) = mpsc::channel::<MqttCommand>(8); //
    let client_clone = client.clone();
    client_clone.subscribe("send", QoS::AtMostOnce).await?;

    let topic_clone = topic.clone();
    let publish_task = task::spawn(async move {
        let mut mission_state: MissionState = MissionState::Idle;
        let mut start_time: Option<std::time::Instant> = None; // 移到这里
        loop {
            tokio::select! {
                // 接收 MAVLink 消息
                Some((header, msg)) = mavlink_rx.recv() => {
                    // 原有消息上报 MQTT（保持不变）
                    if let Err(e) = send_mqtt_data(header, &msg, &client_clone, &topic_clone).await {
                        log::error!("发送 MQTT 数据失败: {}", e);
                    }

                    // 状态机处理
                    mission_state = match msg {
                        MavMessage::MISSION_COUNT(cnt) => {
                            log::info!("📊 收到 MISSION_COUNT: total = {}", cnt.count);
                            match mission_state {
                                MissionState::WaitingCount => {
                                    let total = cnt.count;
                                    if total == 0 {
                                        // 发送空完成消息（可选）
                                        let complete = json!({"type":"waypoints_complete","count":0,"waypoints":[]});
                                        let _ = client_clone.publish("waypoints/response", QoS::AtLeastOnce, false,
                                                                     serde_json::to_vec(&complete).unwrap()).await;
                                        MissionState::Idle
                                    } else {
                                        // 请求第一个航点
                                        let _ = cmd_tx.send(MavlinkCommand::RequestWaypoint(0)).await;
                                        MissionState::Downloading {
                                            expected_count: total,
                                            received: Vec::new(),
                                        }
                                    }
                                }
                                _ => mission_state,
                            }
                        }
                        MavMessage::MISSION_ITEM_INT(item) => {
                            if let MissionState::Downloading { expected_count, mut received } = mission_state {
                                let wp = Waypoint {
                                    seq: item.seq,
                                    lat: item.x as f64 / 1e7,
                                    lon: item.y as f64 / 1e7,
                                    alt: item.z,
                                };
                                received.push(wp.clone());

                                // ✅ 发布单个航点，附带总航点数
                                let wp_json = json!({
                                    "type": "waypoint",
                                    "total_count": expected_count,
                                    "data": wp,
                                });
                                if let Err(e) = client_clone.publish("get_list", QoS::AtLeastOnce, false,
                                                                     serde_json::to_vec(&wp_json).unwrap()).await {
                                    log::error!("MQTT 发送航点失败: {}", e);
                                }

                                let next_seq = item.seq + 1;
                                if next_seq < expected_count {
                                    let _ = cmd_tx.send(MavlinkCommand::RequestWaypoint(next_seq)).await;
                                    MissionState::Downloading { expected_count, received }
                                } else {
                                    // 下载完成，发送汇总消息
                                    let complete = json!({
                                        "type": "waypoints_complete",
                                        "count": received.len(),
                                        "waypoints": received,
                                    });
                                    let _ = client_clone.publish("waypoints/response", QoS::AtLeastOnce, false,
                                                                 serde_json::to_vec(&complete).unwrap()).await;
                                                                 let total = received.len();

                                                                 log::info!("航点下载完成，共 {total} 个", );
                                                                  if let Some(start) = start_time {
                                            let elapsed = start.elapsed();
                                            let rate = total as f64 / elapsed.as_secs_f64();
                                            log::info!(
                                                "⏱️ 读取 {} 个航点耗时: {:.2?}, 平均速度: {:.2} 航点/秒",
                                                total,
                                                elapsed,
                                                rate
                                            );
                                        }

                                    MissionState::Idle
                                }
                            } else {
                                mission_state
                            }
                        }
                        _ => mission_state,
                    };
                }

                // 接收 MQTT 触发命令
                Some(cmd) = mqtt_cmd_rx.recv() => {
                    match cmd {
                        MqttCommand::GetWaypoints => {
                            log::info!("🚀 收到 MQTT 请求，开始航点下载流程");
                            // 发送请求列表命令到工作线程
                              start_time = Some(std::time::Instant::now()); // 在这里赋值
                            let _ = cmd_tx.send(MavlinkCommand::RequestWaypointList).await;
                            mission_state = MissionState::WaitingCount;
                        }
                    }
                }

                else => break,
            }
        }
        log::warn!("MQTT 发布任务结束");
    });
    // 处理 MQTT 事件循环（例如收到订阅确认等）
    loop {
        tokio::select! {
                    event = eventloop.poll() => {
                        match event {
                            Ok(Event::Incoming(Packet::Publish(publish))) => {
                                // 1. 检查 topic
                                if publish.topic == "send" {
                                    // 2. 尝试解析 payload 为 JSON
                                    match serde_json::from_slice::<Payload>(&publish.payload) {
                                        Ok(payload) => {
                                            if payload.r#type == "get_list" {
                                                // 3. 调用函数
                                                log::error!("收到 get_list 请求");
                                                 if let Err(e) = mqtt_cmd_tx.send(MqttCommand::GetWaypoints).await {
                log::error!("发送内部命令失败: {}", e);
            }
        }
                             }
                                        Err(e) => {
                                            log::warn!("解析 payload 失败: {}，原始数据: {:?}", e, publish.payload);
                                        }
                                    }
                                }
                                // 其他 topic 可忽略或处理
                            }
                            Ok(Event::Incoming(other)) => {
                                // 忽略其他类型事件（如 PubAck），或按需记录
                                log::trace!("MQTT 其他事件: {:?}", other);
                            }
                            Ok(Event::Outgoing(_)) => {
                                // 通常忽略 outgoing
                            }
                            Err(e) => {
                                log::error!("MQTT 事件循环错误: {}", e);
                                break;
                            }
                        }
                    }
                    _ = tokio::signal::ctrl_c() => {
                        log::info!("收到退出信号，正在关闭...");
                        break;
                    }
                }
    }
    publish_task.abort();
    Ok(())
}

fn mavlink_worker_thread(
    conn_str: String,
    tx: mpsc::Sender<(MavHeader, MavMessage)>, // 原消息上报通道
    mut cmd_rx: mpsc::Receiver<MavlinkCommand>, // 命令接收通道
) -> Result<()> {
    let vehicle = start_mavlink_thread(conn_str);
    let mut last_heartbeat = std::time::Instant::now();
    // 发送参数请求和数据流请求
    if let Err(e) = vehicle.send_default(&request_parameters()) {
        log::error!("发送参数请求失败: {:?}", e);
    }
    if let Err(e) = vehicle.send_default(&request_stream()) {
        log::error!("发送数据流请求失败: {:?}", e);
    }

    loop {
        // // 1. 定期发送心跳
        if last_heartbeat.elapsed() >= Duration::from_secs(1) {
            if let Err(e) = vehicle.send_default(&heartbeat_message()) {
                log::error!("心跳发送失败: {:?}", e);
            }
            last_heartbeat = std::time::Instant::now();
        }
        match cmd_rx.try_recv() {
            Ok(MavlinkCommand::RequestWaypointList) => {
                log::info!("收到航点列表请求命令");
                let req = MavMessage::MISSION_REQUEST_LIST(MISSION_REQUEST_LIST_DATA {
                    target_system: 1,
                    target_component: 1,
                });
                if let Err(e) = vehicle.send_default(&req) {
                    log::error!("发送 MISSION_REQUEST_LIST 失败: {}", e);
                }
            }
            Ok(MavlinkCommand::RequestWaypoint(seq)) => {
                log::info!("收到航点请求命令，序号: {}", seq);
                let req = MavMessage::MISSION_REQUEST_INT(MISSION_REQUEST_INT_DATA {
                    target_system: 1,
                    target_component: 1,
                    seq,
                });
                if let Err(e) = vehicle.send_default(&req) {
                    log::error!("发送 MISSION_REQUEST_INT 失败: {}", e);
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        }
        // }

        // 3. 接收 MAVLink 消息（设置超时以避免完全阻塞命令响应）
        match vehicle.try_recv() {
            Ok((header, msg)) => {
                log::info!("mavlink 消息:{msg:?}");
                if tx.blocking_send((header, msg)).is_err() {
                    break; // 主线程退出
                }
            }

            Err(MessageReadError::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // 没有新消息，休眠一小段时间避免 CPU 空转
                thread::sleep(Duration::from_millis(1));
            }
            Err(e) => {
                log::error!("接收消息时发生致命错误: {:?}", e);
                break;
            }
        }
    }
    Ok(())
}

fn start_mavlink_thread(
    conn_str: String,
) -> Arc<Box<dyn MavConnection<mavlink::ardupilotmega::MavMessage> + Send + Sync>> {
    let conn = mavlink::connect::<MavMessage>(&conn_str).expect("连接失败");
    log::info!("✅ 已连接到飞控: {}", conn_str);
    // conn
    let vehicle = Arc::new(conn);
    vehicle
}
// 发送mqtt基础数据
async fn send_mqtt_data(
    header: MavHeader,
    msg: &MavMessage,
    client: &AsyncClient,
    topic: &str,
) -> Result<()> {
    let payload = json!({
        "header": {
            "system_id": header.system_id,
            "component_id": header.component_id,
            "sequence": header.sequence,
        },
        "message_type": msg.message_name(),
        "data": msg,
    });
    // log::info!("-----------收到 MAVLink 消息: {payload:?},数据");
    let payload_str = serde_json::to_string(&payload).unwrap_or_else(|e| {
        log::error!("序列化 JSON 失败: {}", e);
        "{}".to_string()
    });
    if let Err(e) = client
        .publish(
            topic.to_string(),
            QoS::AtLeastOnce,
            false,
            payload_str.into_bytes(),
        )
        .await
    {
        log::error!("MQTT 发布失败: {}", e);
    } else {
        log::debug!("已发布消息: {}", msg.message_name());
    }
    Ok(())
}
fn setup_mqtt() -> Result<(AsyncClient, EventLoop)> {
    let mqtt_host = String::from("101.200.223.8");
    let mqtt_port = 1883;
    // 设置 MQTT 连接选项
    let mut mqtt_opts = MqttOptions::new("mavlink_forwarder", mqtt_host, mqtt_port);
    mqtt_opts.set_keep_alive(Duration::from_secs(5));

    // 创建异步 MQTT 客户端和事件循环
    let (client, eventloop) = AsyncClient::new(mqtt_opts, 10);
    Ok((client, eventloop))
}

pub fn heartbeat_message() -> mavlink::ardupilotmega::MavMessage {
    mavlink::ardupilotmega::MavMessage::HEARTBEAT(mavlink::ardupilotmega::HEARTBEAT_DATA {
        custom_mode: 0,
        mavtype: mavlink::ardupilotmega::MavType::MAV_TYPE_QUADROTOR,
        autopilot: mavlink::ardupilotmega::MavAutopilot::MAV_AUTOPILOT_ARDUPILOTMEGA,
        base_mode: mavlink::ardupilotmega::MavModeFlag::empty(),
        system_status: mavlink::ardupilotmega::MavState::MAV_STATE_STANDBY,
        mavlink_version: 0x3,
    })
}

pub fn request_parameters() -> mavlink::ardupilotmega::MavMessage {
    mavlink::ardupilotmega::MavMessage::PARAM_REQUEST_LIST(
        mavlink::ardupilotmega::PARAM_REQUEST_LIST_DATA {
            target_system: 1,
            target_component: 1,
        },
    )
}

pub fn request_stream() -> mavlink::ardupilotmega::MavMessage {
    #[expect(deprecated)]
    mavlink::ardupilotmega::MavMessage::REQUEST_DATA_STREAM(
        mavlink::ardupilotmega::REQUEST_DATA_STREAM_DATA {
            target_system: 1,
            target_component: 1,
            req_stream_id: 0,
            req_message_rate: 10,
            start_stop: 1,
        },
    )
}
