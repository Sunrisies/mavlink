# 项目架构

本项目是一个基于 Rust 的 MAVLink 消息转发系统，用于连接飞控设备并通过 MQTT 协议将消息转发到远程服务器。系统采用异步架构设计，结合了 MAVLink 协议处理和 MQTT 消息队列，实现了高效的消息转发和控制功能。

## 系统架构

### 核心组件

系统主要由以下几个核心组件组成：

1. **MAVLink 连接管理器**：负责与飞控设备建立和维护连接
2. **MAVLink 消息接收器**：持续接收来自飞控的消息
3. **MAVLink Actor**：处理 MAVLink 消息并管理状态机
4. **MQTT 客户端**：与 MQTT 服务器通信，发布和订阅消息
5. **主控制循环**：协调各组件之间的交互

### 消息流程

```
飞控设备 → MAVLink 连接 → MAVLink 接收器 → 主控制循环 → MQTT 客户端 → 远程服务器
                    ↓
                MAVLink Actor
                    ↓
              状态机处理
```

## 模块说明

### 1. MAVLink 连接管理

MAVLink 连接管理器负责与飞控设备建立连接。当前支持 UDP 连接（`udpin:127.0.0.1:23445`），也可以扩展支持串口连接。

```rust
let conn_str = String::from("udpin:127.0.0.1:23445");
let conn = mavlink::connect::<MavMessage>(&conn_str).expect("连接失败");
```

### 2. MAVLink 消息接收器

消息接收器运行在独立的线程中，持续接收来自飞控的消息，并将消息转发到主控制循环和 MAVLink Actor。接收器还负责定期发送心跳消息以保持连接。

```rust
fn mavlink_receiver_thread(
    conn: &Arc<Box<dyn MavConnection<MavMessage> + Send + Sync>>,
    tx: mpsc::Sender<(MavHeader, MavMessage)>,
    actor_tx: mpsc::Sender<MavlinkActorMessage>,
) -> Result<()>
```

### 3. MAVLink Actor

MAVLink Actor 是系统的核心组件，负责处理 MAVLink 消息并管理状态机。它实现了以下功能：

- 航点列表下载和上传
- 飞行器加解锁控制
- 飞行模式切换
- 参数请求

Actor 使用状态机模式管理航点操作的状态：

```rust
pub enum MissionState {
    Idle,
    WaitingCount,
    Downloading {
        expected_count: u16,
        received: Vec<Waypoint>,
    },
    Uploading {
        waypoints: Vec<Waypoint>,
        current_index: usize,
    },
}
```

### 4. MQTT 客户端

MQTT 客户端负责与远程 MQTT 服务器通信。系统使用 `rumqttc` 库实现 MQTT v5 协议的客户端。

主要功能：

- 发布 MAVLink 消息到指定主题
- 订阅控制命令主题
- 处理接收到的控制命令

### 5. 主控制循环

主控制循环使用 `tokio::select!` 宏协调多个异步任务：

- 处理来自 MAVLink 接收器的消息
- 处理来自 MQTT 客户端的事件
- 处理系统信号（如 Ctrl+C）

## 消息类型

系统支持以下类型的控制命令：

```rust
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
```

## 日志系统

项目使用 `log4rs` 实现结构化日志，支持：

- 彩色控制台输出
- 文件日志记录
- 日志文件滚动（按大小）
- 不同级别的日志过滤

## 并发模型

系统采用以下并发模型：

1. **线程池**：使用 `tokio` 运行时管理异步任务
2. **通道**：使用 `tokio::sync::mpsc` 实现组件间通信
3. **Actor 模式**：MAVLink Actor 使用消息传递模式处理请求
4. **独立线程**：MAVLink 接收器运行在独立线程中，避免阻塞主循环

## 扩展性

系统设计考虑了以下扩展性：

1. **支持多种连接类型**：可扩展支持串口、TCP 等连接方式
2. **支持多种飞控协议**：通过 MAVLink 协议支持 ArduPilot、PX4 等多种飞控
3. **支持多种消息类型**：可轻松添加新的 MAVLink 消息处理逻辑
4. **支持多种控制命令**：可扩展添加新的控制命令类型

## 性能考虑

1. **异步 I/O**：使用 `tokio` 实现高效的异步 I/O 操作
2. **消息通道缓冲**：设置合理的通道缓冲区大小，避免阻塞
3. **批量处理**：航点上传/下载支持批量处理，提高效率
4. **日志优化**：使用结构化日志，避免频繁字符串拼接
