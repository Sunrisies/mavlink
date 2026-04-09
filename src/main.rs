mod logger;

use anyhow::Result;
use logger::init_logger;
use mavlink::{
    MavHeader, Message,
    ardupilotmega::{MISSION_REQUEST_INT_DATA, MISSION_REQUEST_LIST_DATA, MavMessage},
};
use rumqttc::v5::{
    AsyncClient, Event, MqttOptions,
    mqttbytes::{QoS, v5::Packet},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    sync::mpsc::{Sender, channel},
    thread,
    time::Duration,
};
use tokio::{task, time};

// 假设 payload 结构如下
#[derive(Debug, Deserialize)]
struct Payload {
    r#type: String,
    // 其他字段...
}

// 自定义业务函数（异步示例）
async fn handle_get_list() {
    log::info!("执行 get_list 操作");
    // 你的具体逻辑...
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
    let mqtt_host = String::from("101.200.223.8");
    let mqtt_port = 1883;
    let topic = String::from("mavlink/incoming");
    // 创建通道：MAVLink 接收线程 -> MQTT 发布任务
    let (tx, rx) = channel::<(MavHeader, MavMessage)>();
    // 启动 MAVLink 接收线程（阻塞）
    let conn_str_owned = conn_str.to_string();
    thread::spawn(move || {
        if let Err(e) = mavlink_receiver_thread(conn_str_owned, tx) {
            log::error!("MAVLink 接收线程崩溃: {}", e);
        }
    });
    // 设置 MQTT 连接选项
    let mut mqtt_opts = MqttOptions::new("mavlink_forwarder", mqtt_host, mqtt_port);
    mqtt_opts.set_keep_alive(Duration::from_secs(5));

    // 创建异步 MQTT 客户端和事件循环
    let (client, mut eventloop) = AsyncClient::new(mqtt_opts, 10);
    let client_clone = client.clone();
    client_clone.subscribe("send", QoS::AtMostOnce).await?;
    /*
     * 创建一个包含闭包的异步任务。
     * 在闭包内部，首先调用 requests(client).await；执行消息发布和订阅操作。
     * 然后，使用 time::sleep(Duration::from_secs(3)).await; 让任务休眠 3 秒。
     */
    // 启动一个异步任务处理通道中的消息并发布到 MQTT
    let publish_task = task::spawn(async move {
        let mut rx = rx;
        while let Ok((header, msg)) = rx.recv() {
            // 将消息转换为 JSON（包含消息类型、header 和内容）
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
            // 发布到 MQTT
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
        }
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
                                        log::info!("收到 get_list 请求");
                                        fetch_waypoints(&conn_str.clone());
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

/// 从飞控接收消息并发送到通道
fn mavlink_receiver_thread(conn_str: String, tx: Sender<(MavHeader, MavMessage)>) -> Result<()> {
    let mut conn = mavlink::connect::<MavMessage>(&conn_str)?;
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

// 航点读取函数（同步阻塞，用于 spawn_blocking）
fn fetch_waypoints(conn_str: &str) -> Result<Vec<Waypoint>, String> {
    log::info!("✅ 已连接到飞控: {}", conn_str);
    let mut conn =
        mavlink::connect::<MavMessage>(conn_str).map_err(|e| format!("连接失败: {}", e))?;
    // 发送 MISSION_REQUEST_LIST
    let req_list = MavMessage::MISSION_REQUEST_LIST(MISSION_REQUEST_LIST_DATA {
        target_system: 1,
        target_component: 1,
    });
    conn.send_default(&req_list)
        .map_err(|e| format!("发送请求失败: {}", e))?;

    // 等待 MISSION_COUNT
    let (_, msg) = conn.recv().map_err(|e| format!("接收失败: {}", e))?;
    let count = match msg {
        MavMessage::MISSION_COUNT(cnt) => cnt.count,
        _ => return Err("未收到 MISSION_COUNT".into()),
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
            .map_err(|e| format!("发送请求失败: {}", e))?;

        // 等待 MISSION_ITEM_INT
        let (_, msg) = conn.recv().map_err(|e| format!("接收失败: {}", e))?;
        match msg {
            MavMessage::MISSION_ITEM_INT(item) => {
                waypoints.push(Waypoint {
                    seq: item.seq,
                    lat: item.x as f64 / 10_000_000.0,
                    lon: item.y as f64 / 10_000_000.0,
                    alt: item.z,
                });
            }
            _ => return Err(format!("期望 MISSION_ITEM_INT，收到其他消息")),
        }
    }
    log::info!("获取到 {} 个航点", waypoints.len());
    Ok(waypoints)
}
