# MQTT 集成指南

本指南介绍项目中 MQTT 协议的集成方式，包括连接管理、消息发布、订阅处理等核心功能。

## MQTT 简介

MQTT (Message Queuing Telemetry Transport) 是一种轻量级的发布/订阅消息协议，广泛应用于物联网和实时通信场景。本项目使用 MQTT 协议实现与远程服务器的通信，将 MAVLink 消息转发到云端，并接收来自云端的控制命令。

## MQTT 客户端配置

### 连接设置

项目使用 `rumqttc` 库实现 MQTT v5 协议的客户端。以下是 MQTT 连接的配置：

```rust
fn setup_mqtt() -> Result<(AsyncClient, EventLoop)> {
    let mqtt_host = String::from("mqtt.example.com");
    let mqtt_port = 1883;

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
```

### 连接参数说明

- **mqtt_host**：MQTT 服务器地址
- **mqtt_port**：MQTT 服务器端口（默认 1883）
- **client_id**：客户端唯一标识，使用时间戳确保唯一性
- **keep_alive**：心跳间隔，设置为 5 秒
- **max_packet_size**：最大消息大小，设置为 10MB

## 消息发布

### 发布 MAVLink 消息

系统将接收到的 MAVLink 消息转换为 JSON 格式后发布到 MQTT 服务器：

```rust
async fn send_mqtt_data(
    header: MavHeader,
    msg: &MavMessage,
    client: &AsyncClient,
    topic: &str,
) -> Result<()> {
    // 将 MAVLink 消息转换为 JSON
    let payload = match msg {
        MavMessage::HEARTBEAT(data) => {
            json!({
                "header": {
                    "system_id": header.system_id,
                    "component_id": header.component_id,
                    "sequence": header.sequence,
                },
                "message_type": "HEARTBEAT",
                "data": {
                    "custom_mode": data.custom_mode,
                    "arm_status": arm_status,
                    "is_armed": is_armed,
                    // ... 其他字段
                }
            })
        }
        // ... 其他消息类型的转换
        _ => {
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

    // 发布到 MQTT
    let payload_str = serde_json::to_string(&payload)?;
    client.publish(topic, QoS::AtLeastOnce, false, payload_str.into_bytes()).await?;
    Ok(())
}
```

### 发布航点信息

系统在航点下载和上传过程中会发布特定的航点信息：

```rust
// 发布航点下载开始通知
let start_json = json!({
    "type": "waypoint_download_start",
    "total": expected_total,
});
client.publish(
    "get_list",
    QoS::AtLeastOnce,
    false,
    serde_json::to_vec(&start_json).unwrap(),
).await?;

// 发布单个航点
let wp_json = json!({
    "type": "waypoint",
    "total_count": expected_total,
    "data": wp,
});
client.publish(
    "get_list",
    QoS::AtLeastOnce,
    false,
    serde_json::to_vec(&wp_json).unwrap(),
).await?;

// 发布航点下载完成通知
let complete_json = json!({
    "type": "waypoints_complete",
    "count": waypoint_received.len(),
    "waypoints": waypoint_received,
});
client.publish(
    "get_list",
    QoS::AtLeastOnce,
    false,
    serde_json::to_vec(&complete_json).unwrap(),
).await?;
```

## 消息订阅

### 订阅控制命令

系统订阅 "send" 主题以接收控制命令：

```rust
let client_clone = client.clone();
client_clone.subscribe("send", QoS::AtMostOnce).await?;
```

### 处理接收到的消息

主循环监听 MQTT 事件，处理接收到的消息：

```rust
loop {
    tokio::select! {
        event = eventloop.poll() => {
            match event {
                Ok(Event::Incoming(Packet::Publish(publish))) => {
                    // 检查 topic
                    if publish.topic == "send" {
                        // 解析 payload 为 JSON
                        match serde_json::from_slice::<Payload>(&publish.payload) {
                            Ok(payload) => {
                                match payload {
                                    Payload::GetList => {
                                        // 处理获取航点列表请求
                                        if let Err(e) = mavlink_actor_tx_clone
                                        .send(MavlinkActorMessage::RequestWaypointList)
                                        .await {
                                            log::error!("发送航点列表请求给 Actor 失败: {}", e);
                                        }
                                    }
                                    Payload::Arm { arm } => {
                                        // 处理加解锁请求
                                        if let Err(e) = mavlink_actor_tx_clone.send(MavlinkActorMessage::ArmDisarm { arm }).await {
                                            log::error!("发送加解锁控制命令失败: {}", e);
                                        }
                                    }
                                    Payload::SetMode { mode } => {
                                        // 处理设置飞行模式请求
                                        if let Err(e) = mavlink_actor_tx.send(MavlinkActorMessage::SetMode { mode }).await {
                                            log::error!("发送设置飞行模式命令失败: {}", e);
                                        }
                                    }
                                    Payload::SetList { data } => {
                                        // 处理设置航点列表请求
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
```

## 消息格式

### 控制命令格式

系统支持以下控制命令格式：

#### 获取航点列表

```json
{
  "type": "get_list"
}
```

#### 加解锁控制

```json
{
  "type": "arm",
  "arm": true
}
```

#### 设置飞行模式

```json
{
  "type": "set_mode",
  "mode": "AUTO"
}
```

#### 设置航点列表

```json
{
  "type": "set_list",
  "data": [
    {
      "seq": 0,
      "lat": 39.9042,
      "lon": 116.4074,
      "alt": 100.0
    },
    {
      "seq": 1,
      "lat": 39.9142,
      "lon": 116.4174,
      "alt": 120.0
    }
  ]
}
```

### MAVLink 消息格式

系统将 MAVLink 消息转换为以下 JSON 格式：

#### 心跳消息

```json
{
  "header": {
    "system_id": 1,
    "component_id": 1,
    "sequence": 123
  },
  "message_type": "HEARTBEAT",
  "data": {
    "custom_mode": 10,
    "flight_mode": "AUTO",
    "vehicle_type": "四旋翼",
    "autopilot": "MAV_AUTOPILOT_ARDUPILOTMEGA",
    "base_mode": {
      "manual_input_enabled": true,
      "custom_mode_enabled": true,
      "auto_enabled": true,
      "guided_enabled": false,
      "stabilize_enabled": true,
      "hil_enabled": false
    },
    "system_status": "活动",
    "mavlink_version": 3,
    "arm_status": "解锁",
    "is_armed": true,
    "mode_type": "自动",
    "is_auto": true,
    "is_standby": false
  }
}
```

#### 位置消息

```json
{
  "header": {
    "system_id": 1,
    "component_id": 1,
    "sequence": 124
  },
  "message_type": "GLOBAL_POSITION_INT",
  "data": {
    "time_boot_ms": 12345,
    "lat": 39.9042,
    "lon": 116.4074,
    "alt": 100.0,
    "relative_alt": 50.0,
    "vx": 1.5,
    "vy": 2.3,
    "vz": 0.5,
    "hdg": 180.0
  }
}
```

#### 姿态消息

```json
{
  "header": {
    "system_id": 1,
    "component_id": 1,
    "sequence": 125
  },
  "message_type": "ATTITUDE",
  "data": {
    "time_boot_ms": 12346,
    "roll": 0.1,
    "pitch": 0.2,
    "yaw": 1.57,
    "rollspeed": 0.01,
    "pitchspeed": 0.02,
    "yawspeed": 0.03
  }
}
```

## 主题结构

系统使用以下 MQTT 主题结构：

### 发布主题

- **mavlink/incoming**：发布接收到的 MAVLink 消息（除航点相关消息外）
- **get_list**：发布航点下载相关信息
- **set_list**：发布航点上传相关信息

### 订阅主题

- **send**：订阅控制命令

## 错误处理

系统对 MQTT 操作进行了完善的错误处理：

```rust
// 发布错误处理
if let Err(e) = client.publish(topic, QoS::AtLeastOnce, false, payload_str.into_bytes()).await {
    log::error!("MQTT 发布失败: {}", e);
} else {
    log::debug!("已发布消息: {}", msg.message_name());
}

// 订阅错误处理
if let Err(e) = client.subscribe("send", QoS::AtMostOnce).await {
    log::error!("MQTT 订阅失败: {}", e);
}

// 解析错误处理
match serde_json::from_slice::<Payload>(&publish.payload) {
    Ok(payload) => {
        // 处理消息
    }
    Err(e) => {
        log::warn!("解析 payload 失败: {}，原始数据: {:?}", e, publish.payload);
    }
}
```

## 性能优化

### QoS 级别选择

系统根据消息类型选择不同的 QoS 级别：

- **控制命令**：使用 `QoS::AtMostOnce`（最多一次），确保低延迟
- **状态消息**：使用 `QoS::AtLeastOnce`（至少一次），确保消息到达
- **航点数据**：使用 `QoS::AtLeastOnce`，确保数据完整性

### 消息大小限制

系统设置了合理的消息大小限制（10MB），避免大消息阻塞网络：

```rust
mqtt_opts.set_max_packet_size(Some(10 * 1024 * 1024));
```

### 异步处理

系统使用异步 I/O 处理 MQTT 消息，避免阻塞主线程：

```rust
let publish_task = task::spawn(async move {
    while let Some((header, msg)) = mavlink_rx.recv().await {
        // 处理并发布消息
    }
});
```

## 安全考虑

### 连接安全

当前实现使用未加密的 MQTT 连接（端口 1883）。在生产环境中，建议：

1. 使用 TLS 加密连接（端口 8883）
2. 实现客户端认证
3. 实现消息加密

### 消息安全

1. 避免在消息中包含敏感信息
2. 实现消息签名和验证
3. 实现消息加密

## 常见问题

### 连接失败

如果 MQTT 连接失败，请检查：

1. MQTT 服务器地址和端口是否正确
2. 网络连接是否正常
3. 防火墙设置是否允许连接

### 消息丢失

如果出现消息丢失，请检查：

1. QoS 级别设置是否合适
2. 网络连接是否稳定
3. 消息大小是否超过限制

### 性能问题

如果出现性能问题，请检查：

1. 消息发布频率是否过高
2. 消息大小是否过大
3. 网络带宽是否充足
