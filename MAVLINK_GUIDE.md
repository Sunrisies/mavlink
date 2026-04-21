# MAVLink 消息处理指南

本指南介绍项目中 MAVLink 消息的处理机制，包括消息接收、解析、转换和转发等核心功能。

## MAVLink 协议简介

MAVLink 是一种轻量级的消息传输协议，广泛用于无人机和机器人系统。本项目使用 Rust 的 `mavlink` crate 来处理 MAVLink 消息，支持 ArduPilot 飞控的消息格式。

## 消息类型

### 核心消息类型

项目主要处理以下 MAVLink 消息类型：

1. **HEARTBEAT**：心跳消息，用于保持连接和获取系统状态
2. **GLOBAL_POSITION_INT**：全局位置信息，包括经纬度、高度和速度
3. **ATTITUDE**：姿态信息，包括横滚、俯仰和偏航角
4. **MISSION_COUNT**：航点数量消息
5. **MISSION_ITEM_INT**：航点详细信息
6. **MISSION_REQUEST**：航点请求消息
7. **MISSION_ACK**：航点操作确认消息
8. **COMMAND_ACK**：命令执行确认消息

## 消息接收

### 连接建立

系统通过以下方式建立与飞控的连接：

```rust
let conn_str = String::from("udpin:127.0.0.1:23445");
let conn = mavlink::connect::<MavMessage>(&conn_str).expect("连接失败");
```

### 接收循环

消息接收器运行在独立线程中，持续接收来自飞控的消息：

```rust
loop {
    // 定期发送心跳
    if last_heartbeat.elapsed() >= Duration::from_secs(1) {
        if let Err(e) = conn.send_default(&heartbeat_message()) {
            log::error!("心跳发送失败: {:?}", e);
        }
        last_heartbeat = std::time::Instant::now();
    }

    // 接收 MAVLink 消息
    match conn.recv() {
        Ok((header, msg)) => {
            // 转发消息到主循环
            if tx.blocking_send((header, msg.clone())).is_err() {
                log::warn!("主通道已关闭，退出接收线程");
                break;
            }

            // 发送消息到 Actor
            if let Err(e) = actor_tx.blocking_send(MavlinkActorMessage::MavlinkMessage((header, msg))) {
                log::error!("发送给 Actor 失败: {}", e);
                break;
            }
        }
        Err(e) => {
            log::error!("接收 MAVLink 消息失败: {:?}", e);
            thread::sleep(Duration::from_millis(100));
        }
    }
}
```

## 消息处理

### 主循环处理

主循环接收来自 MAVLink 接收器的消息，并根据消息类型进行不同处理：

```rust
while let Some((header, msg)) = mavlink_rx.recv().await {
    match msg {
        MavMessage::MISSION_COUNT(cnt) => {
            // 处理航点数量
            expected_total = cnt.count;
            waypoint_received.clear();
        }
        MavMessage::MISSION_ITEM_INT(item) => {
            // 处理航点信息
            let wp = Waypoint {
                seq: item.seq,
                lat: item.x as f64 / 1e7,
                lon: item.y as f64 / 1e7,
                alt: item.z,
            };
            waypoint_received.push(wp.clone());
        }
        MavMessage::MISSION_REQUEST(msg) => {
            // 处理航点请求
            is_uploading = true;
            let wp = WaypointWrite { seq: msg.seq };
        }
        MavMessage::MISSION_ACK(msg) => {
            // 处理航点确认
            if is_uploading {
                is_uploading = false;
                log::info!("航点上传完成");
            }
        }
        _ => {
            // 其他消息发布到 mavlink/incoming 频道
            if let Err(e) = send_mqtt_data(header, &msg, &client_clone, &topic_clone).await {
                log::error!("发送 MQTT 数据失败: {}", e);
            }
        }
    }
}
```

### Actor 处理

MAVLink Actor 使用状态机模式处理消息，特别是航点相关的消息：

```rust
async fn handle_mavlink_message(&mut self, _header: MavHeader, msg: MavMessage) -> Result<()> {
    match msg {
        MavMessage::MISSION_COUNT(cnt) => {
            self.state = match self.state {
                MissionState::WaitingCount => {
                    if cnt.count == 0 {
                        MissionState::Idle
                    } else {
                        // 请求第一个航点
                        let req = MavMessage::MISSION_REQUEST_INT(MISSION_REQUEST_INT_DATA {
                            target_system: 1,
                            target_component: 1,
                            seq: 0,
                        });
                        if let Err(e) = self.vehicle.send_default(&req) {
                            log::error!("发送 MISSION_REQUEST_INT 失败: {}", e);
                            MissionState::Idle
                        } else {
                            MissionState::Downloading {
                                expected_count: cnt.count,
                                received: Vec::new(),
                            }
                        }
                    }
                }
                _ => self.state.clone(),
            };
        }
        MavMessage::MISSION_ITEM_INT(item) => {
            self.state = match std::mem::take(&mut self.state) {
                MissionState::Downloading {
                    expected_count,
                    mut received,
                } => {
                    let wp = Waypoint {
                        seq: item.seq,
                        lat: item.x as f64 / 1e7,
                        lon: item.y as f64 / 1e7,
                        alt: item.z,
                    };
                    received.push(wp);

                    let next_seq = item.seq + 1;
                    if next_seq < expected_count {
                        // 请求下一个航点
                        let req = MavMessage::MISSION_REQUEST_INT(MISSION_REQUEST_INT_DATA {
                            target_system: 1,
                            target_component: 1,
                            seq: next_seq,
                        });
                        if let Err(e) = self.vehicle.send_default(&req) {
                            log::error!("发送下一个 MISSION_REQUEST_INT 失败: {}", e);
                            MissionState::Idle
                        } else {
                            MissionState::Downloading {
                                expected_count,
                                received,
                            }
                        }
                    } else {
                        // 所有航点接收完毕
                        MissionState::Idle
                    }
                }
                _ => self.state.clone(),
            };
        }
        // ... 其他消息处理
        _ => {}
    }
    Ok(())
}
```

## 消息转换

### JSON 转换

系统将 MAVLink 消息转换为 JSON 格式，便于通过 MQTT 传输和前端处理：

```rust
async fn send_mqtt_data(
    header: MavHeader,
    msg: &MavMessage,
    client: &AsyncClient,
    topic: &str,
) -> Result<()> {
    let payload = match msg {
        MavMessage::HEARTBEAT(data) => {
            let is_armed = data.base_mode.contains(MavModeFlag::MAV_MODE_FLAG_SAFETY_ARMED);
            let arm_status = if is_armed { "解锁" } else { "加锁" };

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
        MavMessage::GLOBAL_POSITION_INT(data) => {
            json!({
                "header": {
                    "system_id": header.system_id,
                    "component_id": header.component_id,
                    "sequence": header.sequence,
                },
                "message_type": "GLOBAL_POSITION_INT",
                "data": {
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

## 控制命令

### 加解锁控制

系统支持通过 MQTT 发送加解锁命令：

```rust
async fn handle_arm_disarm(&mut self, arm: bool) -> Result<()> {
    log::info!("收到解锁/上锁命令: {}", arm);
    let msg = MavMessage::COMMAND_LONG(COMMAND_LONG_DATA {
        target_system: 1,
        target_component: 1,
        command: MAV_CMD_COMPONENT_ARM_DISARM,
        confirmation: 1,
        param1: if arm { 1.0 } else { 0.0 },
        param2: 0.0,
        param3: 0.0,
        param4: 0.0,
        param5: 0.0,
        param6: 0.0,
        param7: 0.0,
    });
    if let Err(e) = self.vehicle.send_default(&msg) {
        log::error!("发送解锁/上锁命令失败: {}", e);
    }
    Ok(())
}
```

### 飞行模式切换

系统支持切换不同的飞行模式：

```rust
async fn handle_set_mode(&mut self, mode: String) -> Result<()> {
    log::info!("收到模式切换命令: {}", mode);
    let mode_value = match mode.as_str() {
        "MANUAL" => RoverMode::ROVER_MODE_MANUAL as u16,
        "ACRO" => RoverMode::ROVER_MODE_ACRO as u16,
        "STEERING" => RoverMode::ROVER_MODE_STEERING as u16,
        "HOLD" => RoverMode::ROVER_MODE_HOLD as u16,
        "LOITER" => RoverMode::ROVER_MODE_LOITER as u16,
        "FOLLOW" => RoverMode::ROVER_MODE_FOLLOW as u16,
        "SIMPLE" => RoverMode::ROVER_MODE_SIMPLE as u16,
        "DOCK" => RoverMode::ROVER_MODE_DOCK as u16,
        "CIRCLE" => RoverMode::ROVER_MODE_CIRCLE as u16,
        "AUTO" => RoverMode::ROVER_MODE_AUTO as u16,
        "RTL" => RoverMode::ROVER_MODE_RTL as u16,
        "SMART_RTL" => RoverMode::ROVER_MODE_SMART_RTL as u16,
        "GUIDED" => RoverMode::ROVER_MODE_GUIDED as u16,
        "INITIALISING" => RoverMode::ROVER_MODE_INITIALIZING as u16,
        _ => {
            log::warn!("未知的飞行模式: {}", mode);
            return Ok(());
        }
    };

    let msg = MavMessage::COMMAND_LONG(COMMAND_LONG_DATA {
        target_system: 1,
        target_component: 1,
        command: MAV_CMD_DO_SET_MODE,
        confirmation: 0,
        param1: 1.0,
        param2: mode_value as f32,
        param3: 0.0,
        param4: 0.0,
        param5: 0.0,
        param6: 0.0,
        param7: 0.0,
    });
    if let Err(e) = self.vehicle.send_default(&msg) {
        log::error!("发送模式切换命令失败: {}", e);
    };
    Ok(())
}
```

## 航点管理

### 下载航点列表

系统支持从飞控下载航点列表：

1. 发送 MISSION_REQUEST_LIST 请求
2. 接收 MISSION_COUNT 消息获取航点总数
3. 逐个请求航点（MISSION_REQUEST_INT）
4. 接收航点信息（MISSION_ITEM_INT）
5. 完成后更新状态为 Idle

### 上传航点列表

系统支持向飞控上传航点列表：

1. 发送 MISSION_CLEAR_ALL 清除现有航点
2. 发送 MISSION_COUNT 告知航点总数
3. 接收 MISSION_REQUEST 请求
4. 逐个发送航点（MISSION_ITEM_INT）
5. 接收 MISSION_ACK 确认

## 心跳机制

系统定期发送心跳消息以保持与飞控的连接：

```rust
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
```

## 常见问题

### 1. 消息丢失

如果出现消息丢失，请检查：
- 网络连接是否稳定
- 消息通道缓冲区是否足够大
- 是否有处理瓶颈导致消息积压

### 2. 航点上传失败

如果航点上传失败，请检查：
- 航点数据格式是否正确
- 飞控是否处于可接受航点上传的状态
- 是否有其他任务正在进行

### 3. 心跳超时

如果出现心跳超时，请检查：
- 飞控是否正常运行
- 网络连接是否正常
- 心跳发送间隔是否合理
