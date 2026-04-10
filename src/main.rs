mod logger;

use anyhow::Result;
use logger::init_logger;
use mavlink::{
    MavConnection, MavHeader, Message,
    ardupilotmega::{MavMessage, MavModeFlag},
};
use rumqttc::v5::{
    AsyncClient, Event, EventLoop, MqttOptions,
    mqttbytes::{QoS, v5::Packet},
};
use serde::Deserialize;
use serde_json::json;
use std::{sync::Arc, thread, time::Duration};
use tokio::sync::mpsc::{self};
use tokio::task;

use crate::mavlink_actor::{
    MavlinkActor, MavlinkActorMessage, Waypoint, heartbeat_message, request_parameters,
    request_stream,
};
mod mavlink_actor;

// 定义不同的消息类型
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum Payload {
    #[serde(rename = "get_list")]
    GetList,
    #[serde(rename = "arm")]
    Arm { arm: bool },
    // #[serde(rename = "set_mode")]
    // SetMode { mode: String },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logger();
    let conn_str = String::from("udpin:127.0.0.1:23445");
    let conn_str_clone = conn_str.clone();
    let topic = String::from("mavlink/incoming");
    let (client, mut eventloop) = setup_mqtt()?;
    // 2. 创建消息通道
    // 从 MAVLink 线程到主循环
    let (mavlink_tx, mut mavlink_rx) = mpsc::channel(256);
    // 从主循环到 MAVLink 线程（用于发送命令）
    let (mavlink_actor_tx, mavlink_actor_rx) = mpsc::channel(256);
    let tx_clone = mavlink_tx.clone();
    let actor_tx_clone = mavlink_actor_tx.clone();
    let vehicle = start_mavlink_thread(&conn_str_clone);
    let vehicle_clone = vehicle.clone();
    thread::spawn(move || {
        log::info!("---------");
        if let Err(e) = mavlink_receiver_thread(&vehicle, tx_clone, actor_tx_clone) {
            log::error!("MAVLink 工作线程崩溃: {}", e);
        }
    });
    let mavlink_actor = MavlinkActor::new(vehicle_clone, mavlink_tx);
    let actor_handle = tokio::spawn(async move {
        if let Err(e) = mavlink_actor.run(mavlink_actor_rx).await {
            log::error!("MAVLink Actor 崩溃: {}", e);
        }
    });
    let client_clone = client.clone();
    client_clone.subscribe("send", QoS::AtMostOnce).await?;

    let topic_clone = topic.clone();
    let publish_task = task::spawn(async move {
        let mut waypoint_received: Vec<Waypoint> = Vec::new();
        let mut expected_total = 0;
        while let Some((header, msg)) = mavlink_rx.recv().await {
            match msg {
                MavMessage::MISSION_COUNT(cnt) => {
                    expected_total = cnt.count;
                    log::info!("收到 MISSION_COUNT，总航点数: {}", expected_total);
                    waypoint_received.clear();
                    // 可选：发送一个开始下载的 MQTT 通知
                    let start_json = json!({
                        "type": "waypoint_download_start",
                        "total": expected_total,
                    });
                    let _ = client_clone
                        .publish(
                            "get_list",
                            QoS::AtLeastOnce,
                            false,
                            serde_json::to_vec(&start_json).unwrap(),
                        )
                        .await;
                }
                MavMessage::MISSION_ITEM_INT(item) => {
                    let wp = Waypoint {
                        seq: item.seq,
                        lat: item.x as f64 / 1e7,
                        lon: item.y as f64 / 1e7,
                        alt: item.z,
                    };
                    waypoint_received.push(wp.clone());

                    // 发布单个航点到 get_list 频道
                    let wp_json = json!({
                        "type": "waypoint",
                        "total_count": expected_total,
                        "data": wp,
                    });
                    if let Err(e) = client_clone
                        .publish(
                            "get_list",
                            QoS::AtLeastOnce,
                            false,
                            serde_json::to_vec(&wp_json).unwrap(),
                        )
                        .await
                    {
                        log::error!("MQTT 发布航点失败: {}", e);
                    }

                    // 如果是最后一个航点，发布完成消息
                    if (item.seq + 1) == expected_total {
                        let complete_json = json!({
                            "type": "waypoints_complete",
                            "count": waypoint_received.len(),
                            "waypoints": waypoint_received,
                        });
                        let _ = client_clone
                            .publish(
                                "get_list",
                                QoS::AtLeastOnce,
                                false,
                                serde_json::to_vec(&complete_json).unwrap(),
                            )
                            .await;
                        log::info!("航点下载完成，共 {} 个", waypoint_received.len());
                        // 可选：输出耗时（如果需要，可以在外部记录 start_time 并通过通道传入）
                    }
                }
                _ => {
                    // 其他消息发布到原来的 mavlink/incoming 频道
                    if let Err(e) = send_mqtt_data(header, &msg, &client_clone, &topic_clone).await
                    {
                        log::error!("发送 MQTT 数据失败: {}", e);
                    }
                }
            }
        }

        log::warn!("MQTT 发布任务结束");
    });
    let mavlink_actor_tx_clone = mavlink_actor_tx.clone();
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
                                    match payload {
                                        Payload::GetList => {
                                            log::error!("收到 get_list 请求");
                                            if let Err(e) = mavlink_actor_tx_clone
                                            .send(MavlinkActorMessage::RequestWaypointList)
                                            .await
                                            {
                                                log::error!("发送航点列表请求给 Actor 失败: {}", e);
                                            }
                                        }
                                        Payload::Arm { arm } => {
                                            // 处理机臂控制请求
                                            log::info!("收到 arm 请求: {}", arm);
                                            if let Err(e) = mavlink_actor_tx_clone.send(MavlinkActorMessage::ArmDisarm { arm }).await {
                                                log::error!("发送加解锁控制命令失败: {}", e);
                                            }
                                        }
                                        // Payload::SetMode { mode } => {
                                        //     // 处理设置飞行模式请求
                                        //     log::info!("收到 set_mode 请求: {}", mode);
                                        //     if let Err(e) = mavlink_actor_tx.send(MqttCommand::SetMode { mode }).await {
                                        //         log::error!("发送设置飞行模式命令失败: {}", e);
                                        //     }
                                        // }
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
    actor_handle.abort();
    Ok(())
}
/// 从飞控接收消息并发送到通道
fn mavlink_receiver_thread(
    conn: &Arc<Box<dyn MavConnection<MavMessage> + Send + Sync>>,
    tx: mpsc::Sender<(MavHeader, MavMessage)>,
    actor_tx: mpsc::Sender<MavlinkActorMessage>,
) -> Result<()> {
    let mut last_heartbeat = std::time::Instant::now();
    if let Err(e) = conn.send_default(&request_parameters()) {
        log::error!("发送参数请求失败: {:?}", e);
    }
    if let Err(e) = conn.send_default(&request_stream()) {
        log::error!("发送数据流请求失败: {:?}", e);
    }

    loop {
        // 1. 定期发送心跳
        if last_heartbeat.elapsed() >= Duration::from_secs(1) {
            if let Err(e) = conn.send_default(&heartbeat_message()) {
                log::error!("心跳发送失败: {:?}", e);
            }
            last_heartbeat = std::time::Instant::now();
        }

        // 2. 接收 MAVLink 消息
        match conn.recv() {
            Ok((header, msg)) => {
                // 转发消息到主循环
                if tx.blocking_send((header, msg.clone())).is_err() {
                    log::warn!("主通道已关闭，退出接收线程");
                    break;
                }

                // 发送消息到 Actor
                // 发送给 Actor（状态机）
                if let Err(e) =
                    actor_tx.blocking_send(MavlinkActorMessage::MavlinkMessage((header, msg)))
                {
                    log::error!("发送给 Actor 失败: {}", e);
                    break;
                }
            }
            Err(e) => {
                log::error!("接收 MAVLink 消息失败: {:?}", e);
                // 短暂休眠后继续，避免疯狂重试
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
    Ok(())
}

fn start_mavlink_thread(
    conn_str: &String,
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
    // 将 MAVLink 消息转换为前端可理解的 JSON
    let payload = match msg {
        MavMessage::HEARTBEAT(data) => {
            // 解析加锁/解锁状态
            let is_armed = data
                .base_mode
                .contains(MavModeFlag::MAV_MODE_FLAG_SAFETY_ARMED);
            let arm_status = if is_armed { "解锁" } else { "加锁" };

            // 解析自动/手动模式
            let is_auto = data
                .base_mode
                .contains(MavModeFlag::MAV_MODE_FLAG_AUTO_ENABLED);
            let mode_type = if is_auto { "自动" } else { "手动" };

            // 解析待机模式
            let is_standby = !is_armed && !is_auto;

            // 解析具体飞行模式
            let flight_mode = match data.mavtype {
                mavlink::ardupilotmega::MavType::MAV_TYPE_QUADROTOR => "四旋翼",
                mavlink::ardupilotmega::MavType::MAV_TYPE_GROUND_ROVER => "地面车辆",
                mavlink::ardupilotmega::MavType::MAV_TYPE_FIXED_WING => "固定翼",
                mavlink::ardupilotmega::MavType::MAV_TYPE_COAXIAL => "共轴直升机",
                _ => "未知类型",
            };

            // 解析系统状态
            let system_status = match data.system_status {
                mavlink::ardupilotmega::MavState::MAV_STATE_UNINIT => "未初始化",
                mavlink::ardupilotmega::MavState::MAV_STATE_BOOT => "启动中",
                mavlink::ardupilotmega::MavState::MAV_STATE_CALIBRATING => "校准中",
                mavlink::ardupilotmega::MavState::MAV_STATE_STANDBY => "待机",
                mavlink::ardupilotmega::MavState::MAV_STATE_ACTIVE => "活动",
                mavlink::ardupilotmega::MavState::MAV_STATE_CRITICAL => "严重故障",
                mavlink::ardupilotmega::MavState::MAV_STATE_EMERGENCY => "紧急状态",
                _ => "未知状态",
            };

            json!({
                "header": {
                    "system_id": header.system_id,
                    "component_id": header.component_id,
                    "sequence": header.sequence,
                },
                "message_type": "HEARTBEAT",
                "data": {
                    "custom_mode": data.custom_mode,
                    "mavtype": flight_mode,
                    "autopilot": format!("{:?}", data.autopilot),
                    "base_mode": {
                        "manual_input_enabled": data.base_mode.contains(MavModeFlag::MAV_MODE_FLAG_MANUAL_INPUT_ENABLED),
                        "custom_mode_enabled": data.base_mode.contains(MavModeFlag::MAV_MODE_FLAG_CUSTOM_MODE_ENABLED),
                        "auto_enabled": is_auto,
                        "guided_enabled": data.base_mode.contains(MavModeFlag::MAV_MODE_FLAG_GUIDED_ENABLED),
                        "stabilize_enabled": data.base_mode.contains(MavModeFlag::MAV_MODE_FLAG_STABILIZE_ENABLED),
                        "hil_enabled": data.base_mode.contains(MavModeFlag::MAV_MODE_FLAG_HIL_ENABLED),
                    },
                    "system_status": system_status,
                    "mavlink_version": data.mavlink_version,
                    // 新增字段
                    "arm_status": arm_status,
                    "is_armed": is_armed,
                    "mode_type": mode_type,
                    "is_auto": is_auto,
                    "is_standby": is_standby,
                }
            })
        }
        MavMessage::GLOBAL_POSITION_INT(data) => {
            json!({
                "header": {
                    "system_id": header.system_id,
                    "component_id": header.component_id,
                    "sequence": header.sequence,
                },
                "message_type": "GLOBAL_POSITION_INT",
                "data": {
                    "time_boot_ms": data.time_boot_ms,
                    "lat": data.lat as f64 / 1e7,
                    "lon": data.lon as f64 / 1e7,
                    "alt": data.alt as f32 / 1000.0,
                    "relative_alt": data.relative_alt as f32 / 1000.0,
                    "vx": data.vx as f32 / 100.0,
                    "vy": data.vy as f32 / 100.0,
                    "vz": data.vz as f32 / 100.0,
                    "hdg": data.hdg as f32 / 100.0,
                }
            })
        }
        MavMessage::ATTITUDE(data) => {
            json!({
                "header": {
                    "system_id": header.system_id,
                    "component_id": header.component_id,
                    "sequence": header.sequence,
                },
                "message_type": "ATTITUDE",
                "data": {
                    "time_boot_ms": data.time_boot_ms,
                    "roll": data.roll,
                    "pitch": data.pitch,
                    "yaw": data.yaw,
                    "rollspeed": data.rollspeed,
                    "pitchspeed": data.pitchspeed,
                    "yawspeed": data.yawspeed,
                }
            })
        }
        _ => {
            // 对于其他消息类型，使用通用转换
            json!({
                "header": {
                    "system_id": header.system_id,
                    "component_id": header.component_id,
                    "sequence": header.sequence,
                },
                "message_type": msg.message_name(),
                "data": format!("{:?}", msg),
            })
        }
    };
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
