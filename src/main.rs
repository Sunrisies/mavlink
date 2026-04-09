mod logger;

use anyhow::Result;
use logger::init_logger;
use mavlink::{
    MavConnection, MavHeader, Message,
    ardupilotmega::{MISSION_REQUEST_INT_DATA, MISSION_REQUEST_LIST_DATA, MavMessage},
    error::MessageReadError,
};
use rumqttc::{
    tokio_rustls::rustls::client,
    v5::{
        AsyncClient, Event, EventLoop, MqttOptions,
        mqttbytes::{QoS, v5::Packet},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    sync::{Arc, mpsc::Sender},
    thread,
    time::Duration,
};
use tokio::sync::{
    mpsc::{self, Receiver, error::TryRecvError},
    oneshot,
};
use tokio::{task, time};
#[derive(Debug)]
enum MavlinkCommand {
    GetWaypoints {
        reply_tx: oneshot::Sender<Result<Vec<Waypoint>>>,
    },
    RequestWaypointList, // 请求航点列表
    RequestWaypoint(u16), // 请求指定序号的航点
                         // 可以添加其他命令类型
}

// 在主函数或顶层定义状态
enum MissionState {
    Idle,
    WaitingCount {
        reply_tx: oneshot::Sender<Result<Vec<Waypoint>>>,
    }, // 如果还需要返回全部结果
    Downloading {
        expected_count: u16,
        received: Vec<Waypoint>,
        reply_tx: oneshot::Sender<Result<Vec<Waypoint>>>,
    },
}
// 假设 payload 结构如下
#[derive(Debug, Deserialize)]
struct Payload {
    r#type: String,
    // 其他字段...
}

// 定义航点结构体，用于序列化响应
#[derive(Debug, Serialize, Deserialize)]
struct Waypoint {
    seq: u16,
    lat: f64,
    lon: f64,
    alt: f32,
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logger();
    let conn_str = String::from("udpin:127.0.0.1:23445");
    let topic = String::from("mavlink/incoming");
    let (client, mut eventloop) = setup_mqtt()?;
    // 2. 创建消息通道
    // 从 MAVLink 线程到主循环
    let (mavlink_tx, mut mavlink_rx) = mpsc::channel(256);
    // 从主循环到 MAVLink 发送任务
    let (send_tx, mut send_rx) = mpsc::channel::<MavMessage>(256);
    // 从主循环到 MAVLink 线程（用于发送命令）
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<MavlinkCommand>(256);
    let tx_clone = mavlink_tx.clone();
    thread::spawn(move || {
        log::info!("---------");
        if let Err(e) = mavlink_worker_thread(conn_str, tx_clone, cmd_rx) {
            log::error!("MAVLink 工作线程崩溃: {}", e);
        }
    });

    // 在 main 函数中，创建一个通道用于传递航点数据
    let (waypoint_tx, mut waypoint_rx) = mpsc::channel::<Waypoint>(256);

    let client_clone = client.clone();
    client_clone.subscribe("send", QoS::AtMostOnce).await?;

    let topic_clone = topic.clone();
    let cmd_tx_clone = cmd_tx.clone();
    let client_clone_value = client.clone();
    let publish_task = task::spawn(async move {
        // 定义并初始化 mission_state
        let mut mission_state = MissionState::Idle;

        while let Some((header, msg)) = mavlink_rx.recv().await {
            if let Err(e) = send_mqtt_data(header, &msg, &client_clone, &topic_clone).await {
                log::error!("发送 MQTT 数据失败: {}", e);
            }

            match msg {
                MavMessage::HEARTBEAT(_) => {
                    log::info!("💓 收到飞控心跳，连接正常");
                }
                MavMessage::SYSTEM_TIME(msg) => {
                    log::info!("🕒 SYSTEM_TIME: {:?}", msg);
                }
                MavMessage::MISSION_COUNT(cnt) => {
                    log::info!("📊 收到 MISSION_COUNT: total = {}", cnt.count);
                    // 使用 match 来处理不同的状态
                    mission_state = match mission_state {
                        MissionState::WaitingCount { reply_tx } => {
                            let total = cnt.count;
                            log::info!("航点总数: {}", total);
                            if total == 0 {
                                let _ = reply_tx.send(Ok(vec![]));
                                MissionState::Idle
                            } else {
                                // 请求第一个航点
                                let _ = cmd_tx.send(MavlinkCommand::RequestWaypoint(0)).await;
                                MissionState::Downloading {
                                    expected_count: total,
                                    received: Vec::new(),
                                    reply_tx,
                                }
                            }
                        }
                        _ => mission_state,
                    };
                }
                MavMessage::MISSION_ITEM_INT(item) => {
                    // 使用 match 来处理不同的状态
                    mission_state = match mission_state {
                        MissionState::Downloading {
                            expected_count,
                            mut received,
                            reply_tx,
                        } => {
                            let wp = Waypoint {
                                seq: item.seq,
                                lat: item.x as f64 / 1e7,
                                lon: item.y as f64 / 1e7,
                                alt: item.z,
                            };
                            log::info!("收到航点 seq={}", item.seq);

                            // 立即通过 MQTT 发送单个航点
                            let wp_json = json!({ "type": "waypoint", "data": wp });
                            if let Err(e) = client
                                .publish(
                                    "get_list",
                                    QoS::AtLeastOnce,
                                    false,
                                    serde_json::to_vec(&wp_json).unwrap(),
                                )
                                .await
                            {
                                log::error!("MQTT 发送航点失败: {}", e);
                            }

                            received.push(wp);

                            let next_seq = item.seq + 1;
                            if next_seq < expected_count {
                                let _ =
                                    cmd_tx.send(MavlinkCommand::RequestWaypoint(next_seq)).await;
                                MissionState::Downloading {
                                    expected_count,
                                    received,
                                    reply_tx,
                                }
                            } else {
                                // 全部完成
                                let result = Ok(received);
                                let _ = reply_tx.send(result);
                                MissionState::Idle
                            }
                        }
                        _ => mission_state,
                    };

                    if matches!(mission_state, MissionState::Idle) {
                        log::info!("航点下载完成");
                    }
                }
                _ => {
                    // 处理其他消息类型
                }
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
                                        let (reply_tx, reply_rx) = oneshot::channel();

                    if let Err(e) = cmd_tx_clone.send(MavlinkCommand::GetWaypoints { reply_tx }).await {
                        log::error!("发送命令失败: {}", e);
                    } else {
                        // 等待结果
                        match reply_rx.await {
                            Ok(Ok(waypoints)) => {
                                log::info!("成功获取 {} 个航点", waypoints.len());
                                // 将航点数据转换为 JSON 并发布到 MQTT
                                for waypoint in waypoints {
                                    let waypoint_json = json!({
                                        "type": "waypoint",
                                        "data": waypoint
                                    });
                                    log::info!("---------------{waypoint_json}");
                                    if let Err(e) = client_clone_value.publish(
                                        "get_list".to_string(),
                                        QoS::AtLeastOnce,
                                        false,
                                        serde_json::to_vec(&waypoint_json).unwrap_or_default(),
                                    ).await {
                                        log::error!("发布航点数据失败: {}", e);
                                    }
                                }
                            }
                            Ok(Err(e)) => {
                                log::error!("获取航点失败: {}", e);
                            }
                            Err(e) => {
                                log::error!("等待响应失败: {}", e);
                            }
                        }
                    }
                                    } else {
                                        log::debug!("type 不为 get_list: {:?}", payload);
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
fn fetch_waypoints_with_conn(
    conn: &Box<dyn MavConnection<MavMessage> + Send + Sync>,
) -> Result<Vec<Waypoint>> {
    // 发送 MISSION_REQUEST_LIST
    let request_list = MavMessage::MISSION_REQUEST_LIST(MISSION_REQUEST_LIST_DATA {
        target_system: 1,
        target_component: 1,
    });
    log::info!("发送 MISSION_REQUEST_LIST");
    conn.send_default(&request_list).expect("msg");

    // 等待 MISSION_COUNT
    let (_, msg) = conn.recv()?;
    let count = match msg {
        MavMessage::MISSION_COUNT(cnt) => cnt.count,
        _ => return Err(anyhow::anyhow!("未收到 MISSION_COUNT")),
    };

    let mut waypoints = Vec::new();
    for seq in 0..count {
        // 请求单个航点
        let req = MavMessage::MISSION_REQUEST_INT(MISSION_REQUEST_INT_DATA {
            target_system: 1,
            target_component: 1,
            seq,
        });
        conn.send_default(&req)?;

        // 等待 MISSION_ITEM_INT
        let (_, msg) = conn.recv()?;
        match msg {
            MavMessage::MISSION_ITEM_INT(item) => {
                waypoints.push(Waypoint {
                    seq: item.seq,
                    lat: item.x as f64 / 10_000_000.0,
                    lon: item.y as f64 / 10_000_000.0,
                    alt: item.z,
                });
            }
            _ => return Err(anyhow::anyhow!("期望 MISSION_ITEM_INT，收到其他消息")),
        }
    }
    log::info!("获取到 {} 个航点", waypoints.len());
    Ok(waypoints)
}

fn mavlink_worker_thread(
    conn_str: String,
    tx: mpsc::Sender<(MavHeader, MavMessage)>, // 原消息上报通道
    mut cmd_rx: mpsc::Receiver<MavlinkCommand>, // 命令接收通道
) -> Result<()> {
    let vehicle = start_mavlink_thread(conn_str);
    let hb_vehicle = vehicle.clone();
    let mut last_heartbeat = std::time::Instant::now();
    // 发送参数请求和数据流请求
    if let Err(e) = vehicle.send_default(&request_parameters()) {
        log::error!("发送参数请求失败: {:?}", e);
    }
    if let Err(e) = vehicle.send_default(&request_stream()) {
        log::error!("发送数据流请求失败: {:?}", e);
    }
    // 状态机相关
    enum State {
        Idle,
        DownloadingWaypoints {
            expected_count: u16,
            received_waypoints: Vec<Waypoint>,
            reply_tx: oneshot::Sender<Result<Vec<Waypoint>>>,
        },
    }
    let mut state = State::Idle;

    let mavlink_clone_for_blocking = vehicle.clone();
    loop {
        // // 1. 定期发送心跳
        if last_heartbeat.elapsed() >= Duration::from_secs(1) {
            if let Err(e) = vehicle.send_default(&heartbeat_message()) {
                log::error!("心跳发送失败: {:?}", e);
            }
            last_heartbeat = std::time::Instant::now();
        }
        // 1. 检查新命令（仅在空闲时接受）
        if matches!(state, State::Idle) {
            match cmd_rx.try_recv() {
                Ok(MavlinkCommand::GetWaypoints { reply_tx }) => {
                    log::info!("收到航点下载命令，发送 MISSION_REQUEST_LIST");
                    let req = MavMessage::MISSION_REQUEST_LIST(MISSION_REQUEST_LIST_DATA {
                        target_system: 1,
                        target_component: 1,
                    });
                    if let Err(e) = vehicle.send_default(&req) {
                        log::error!("发送 MISSION_REQUEST_LIST 失败: {}", e);
                        let _ = reply_tx.send(Err(anyhow::anyhow!("发送请求失败")));
                    } else {
                        // 进入等待 MISSION_COUNT 的状态，但这里我们实际上需要等待第一个响应
                        // 为了简化，直接在状态机中处理后续消息
                        state = State::DownloadingWaypoints {
                            expected_count: 0, // 暂时未知
                            received_waypoints: Vec::new(),
                            reply_tx,
                        };
                    }
                }
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
        }

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
                thread::sleep(Duration::from_millis(10));
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
    let (client, mut eventloop) = AsyncClient::new(mqtt_opts, 10);
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
/// 从飞控接收消息并发送到通道
fn mavlink_receiver_thread(conn_str: String, tx: Sender<(MavHeader, MavMessage)>) -> Result<()> {
    let mut conn: Box<dyn MavConnection<MavMessage> + Send + Sync> =
        mavlink::connect::<MavMessage>(&conn_str)?;
    log::info!("✅ 已连接到飞控: {}", conn_str);

    // 可选：发送心跳以保持连接（飞控可能要求）
    // 这里简单循环接收
    loop {
        match conn.recv() {
            Ok((header, msg)) => {
                // log::info!("收到 MAVLink 消息: {msg:?}");
                if let Err(e) = tx.send((header, msg)) {
                    log::error!("发送消息到通道失败: {}", e);
                    break;
                }
            }
            Err(e) => {
                log::error!("接收 MAVLink 消息失败: {}", e);
                // 短暂休眠后继续，避免疯狂重试
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
    Ok(())
}

fn fetch_waypoints<F>(
    conn: &Arc<Box<dyn MavConnection<MavMessage> + Send + Sync>>,
    mut on_waypoint: F,
) -> Result<()>
where
    F: FnMut(&Waypoint) -> Result<()>,
{
    // 发送 MISSION_REQUEST_LIST
    let req_list = MavMessage::MISSION_REQUEST_LIST(MISSION_REQUEST_LIST_DATA {
        target_system: 1,
        target_component: 1,
    });
    conn.send_default(&req_list)
        .map_err(|e| anyhow::anyhow!("发送请求失败: {}", e))?;

    // 等待 MISSION_COUNT
    let (_, msg) = conn
        .recv()
        .map_err(|e| anyhow::anyhow!("接收失败: {}", e))?;
    let count = match msg {
        MavMessage::MISSION_COUNT(cnt) => cnt.count,
        _ => return Err(anyhow::anyhow!("未收到 MISSION_COUNT")),
    };

    let mut waypoints = Vec::new();
    for seq in 0..count {
        // 请求单个航点
        let req = MavMessage::MISSION_REQUEST_INT(MISSION_REQUEST_INT_DATA {
            target_system: 1,
            target_component: 1,
            seq,
        });
        conn.send_default(&req)
            .map_err(|e| anyhow::anyhow!("发送请求失败: {}", e))?;

        // 等待 MISSION_ITEM_INT
        let (_, msg) = conn
            .recv()
            .map_err(|e| anyhow::anyhow!("接收失败: {}", e))?;
        match msg {
            MavMessage::MISSION_ITEM_INT(item) => {
                let waypoint = Waypoint {
                    seq: item.seq,
                    lat: item.x as f64 / 10_000_000.0,
                    lon: item.y as f64 / 10_000_000.0,
                    alt: item.z,
                };

                // 调用回调函数发送航点数据
                on_waypoint(&waypoint)?;

                waypoints.push(waypoint);
            }
            _ => return Err(anyhow::anyhow!("期望 MISSION_ITEM_INT，收到其他消息")),
        }
    }
    log::info!("获取到 {} 个航点", waypoints.len());
    Ok(())
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
