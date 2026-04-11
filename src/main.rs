mod logger;

use anyhow::Result;
use logger::init_logger;
use mavlink::{
    MavConnection, MavHeader, Message,
    ardupilotmega::{MavAutopilot, MavMessage, MavModeFlag, MavState, MavType},
};
use rumqttc::v5::{
    AsyncClient, Event, EventLoop, MqttOptions,
    mqttbytes::{QoS, v5::Packet},
};
use serde::Deserialize;
use serde_json::json;
use std::{
    sync::Arc,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::mpsc::{self};
use tokio::task;

use crate::mavlink_actor::{
    MavlinkActor, MavlinkActorMessage, Waypoint, WaypointWrite, heartbeat_message,
    request_parameters, request_stream,
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
    #[serde(rename = "set_mode")]
    SetMode { mode: String },
    #[serde(rename = "set_list")]
    SetList { data: Vec<Waypoint> },
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
        let mut is_uploading = false; // 标记是否正在上传航点
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
                    log::info!("is_uploading:{is_uploading},{wp:?}");
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
                // 处理上传开始通知
                MavMessage::MISSION_CLEAR_ALL(_) => {}
                MavMessage::MISSION_REQUEST(msg) => {
                    is_uploading = true;
                    log::info!("主线程收到航点写入数据{msg:?}");
                    let wp = WaypointWrite { seq: msg.seq };
                    if let Err(e) = client_clone
                        .publish(
                            "set_list",
                            QoS::AtLeastOnce,
                            false,
                            serde_json::to_vec(&wp).unwrap(),
                        )
                        .await
                    {
                        log::error!("MQTT 发布上传航点失败: {}", e);
                    }
                }
                MavMessage::MISSION_ACK(msg) => {
                    if is_uploading {
                        is_uploading = false;
                        let complete_json = json!({
                            "data": "航线写入完成",
                        });
                        let _ = client_clone
                            .publish(
                                "set_list",
                                QoS::AtLeastOnce,
                                false,
                                serde_json::to_vec(&complete_json).unwrap(),
                            )
                            .await;
                        log::info!("航点上传完成");
                    }
                    log::info!("主线程收到航点写入确认{msg:?}");
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
                                        Payload::SetMode { mode } => {
                                            // 处理设置飞行模式请求
                                            log::info!("收到 set_mode 请求: {}", mode);
                                            if let Err(e) = mavlink_actor_tx.send(MavlinkActorMessage::SetMode { mode }).await {
                                                log::error!("发送设置飞行模式命令失败: {}", e);
                                            }
                                        }
                                        Payload::SetList { data } => {
                                            // 处理设置航点列表请求
                                            log::info!("收到 set_list 请求，共 {} 个航点", data.len());
                                            if let Err(e) = mavlink_actor_tx.send(MavlinkActorMessage::SetWaypointList { waypoints: data }).await {
                                                log::error!("发送设置航点列表命令失败: {}", e);
                                            }
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
            let is_armed = data
                .base_mode
                .contains(MavModeFlag::MAV_MODE_FLAG_SAFETY_ARMED);
            let arm_status = if is_armed { "解锁" } else { "加锁" };

            let is_auto = data
                .base_mode
                .contains(MavModeFlag::MAV_MODE_FLAG_AUTO_ENABLED);
            let mode_type = if is_auto { "自动" } else { "手动" };
            let is_standby = !is_armed && !is_auto;

            // 获取飞行模式名称（根据 custom_mode 和 autopilot 类型）
            let flight_mode_name = match data.autopilot {
                MavAutopilot::MAV_AUTOPILOT_ARDUPILOTMEGA => {
                    // ArduPilot 模式映射
                    match data.custom_mode {
                        0 => "MANUAL",
                        1 => "ACRO",
                        3 => "STEERING",
                        4 => "HOLD",
                        5 => "LOITER",
                        6 => "FOLLOW",
                        7 => "SIMPLE",
                        8 => "DOCK",
                        9 => "CIRCLE",
                        10 => "AUTO",
                        11 => "RTL",
                        12 => "SMART_RTL",
                        15 => "GUIDED",
                        16 => "INITIALISING",
                        _ => "UNKNOWN",
                    }
                }
                // 可以添加 PX4 等其他 autopilot 的映射
                _ => "UNKNOWN",
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
                    "flight_mode": flight_mode_name,          // 新增：可读模式名称
                    "vehicle_type": match data.mavtype {      // 原来的 mavtype 改名为 vehicle_type
                        MavType::MAV_TYPE_QUADROTOR => "四旋翼",
                        MavType::MAV_TYPE_GROUND_ROVER => "地面车辆",
                        MavType::MAV_TYPE_FIXED_WING => "固定翼",
                        MavType::MAV_TYPE_COAXIAL => "共轴直升机",
                        _ => "未知类型",
                    },
                    "autopilot": format!("{:?}", data.autopilot),
                    "base_mode": {
                        "manual_input_enabled": data.base_mode.contains(MavModeFlag::MAV_MODE_FLAG_MANUAL_INPUT_ENABLED),
                        "custom_mode_enabled": data.base_mode.contains(MavModeFlag::MAV_MODE_FLAG_CUSTOM_MODE_ENABLED),
                        "auto_enabled": is_auto,
                        "guided_enabled": data.base_mode.contains(MavModeFlag::MAV_MODE_FLAG_GUIDED_ENABLED),
                        "stabilize_enabled": data.base_mode.contains(MavModeFlag::MAV_MODE_FLAG_STABILIZE_ENABLED),
                        "hil_enabled": data.base_mode.contains(MavModeFlag::MAV_MODE_FLAG_HIL_ENABLED),
                    },
                    "system_status": match data.system_status {
                        MavState::MAV_STATE_UNINIT => "未初始化",
                        MavState::MAV_STATE_BOOT => "启动中",
                        MavState::MAV_STATE_CALIBRATING => "校准中",
                        MavState::MAV_STATE_STANDBY => "待机",
                        MavState::MAV_STATE_ACTIVE => "活动",
                        MavState::MAV_STATE_CRITICAL => "严重故障",
                        MavState::MAV_STATE_EMERGENCY => "紧急状态",
                        _ => "未知状态",
                    },
                    "mavlink_version": data.mavlink_version,
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
            log::debug!("msg:{msg:?}");
            // 对于其他消息类型，使用通用转换
            let data_value = serde_json::to_value(msg)?;
            json!({
                "header": {
                    "system_id": header.system_id,
                    "component_id": header.component_id,
                    "sequence": header.sequence,
                },
                "message_type": msg.message_name(),
                "data": data_value,
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
    // 生成随机数作为客户端 ID 的一部分
    let client_id = format!(
        "mavlink_forwarder_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let mut mqtt_opts = MqttOptions::new(client_id, mqtt_host, mqtt_port);
    mqtt_opts.set_keep_alive(Duration::from_secs(5));
    mqtt_opts.set_max_packet_size(Some(10 * 1024 * 1024));
    // 创建异步 MQTT 客户端和事件循环
    let (client, eventloop) = AsyncClient::new(mqtt_opts, 10);
    Ok((client, eventloop))
}
