use mavlink::ardupilotmega::{
    MISSION_COUNT_DATA, MISSION_ITEM_INT_DATA, MISSION_REQUEST_INT_DATA, MavCmd, MavFrame,
    MavMessage, MavMissionResult,
};
use mavlink::error::MessageReadError;
use mavlink::{MavConnection, ardupilotmega::MISSION_REQUEST_LIST_DATA};
use std::sync::Mutex;
use std::thread::sleep;
use std::{env, sync::Arc, thread, time::Duration};
mod logger;
use logger::init_logger;
use rand::{prelude::*, rng};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

// -----------------------------------------------------------------------------
// 航点请求状态机
// -----------------------------------------------------------------------------
#[derive(Debug, PartialEq)]
enum MissionState {
    Idle,
    WaitingCount, // 已发送 MISSION_REQUEST_LIST，等待 MISSION_COUNT
    Requesting { seq: u16, total: u16 }, // 正在请求第 seq 个航点（0‑based）
    Uploading,    // 正在上传航点
}
#[derive(PartialEq)]
enum ProgramMode {
    ReadOnly,       // 只读航点
    UploadThenRead, // 先上传随机航点，然后读取验证
}

fn main() {
    init_logger();
    let args: Vec<_> = env::args().collect();
    if args.len() < 2 {
        log::info!("Usage: mavlink-dump udpin:IP:PORT");
        log::info!("Example: mavlink-dump udpin:127.0.0.1:14550");
        return;
    }

    let conn_str = &args[1];
    let is_upload = args.len() >= 3 && args[2] == "upload";

    let result = if is_upload {
        upload_random_waypoints(conn_str)
    } else {
        read_waypoints(conn_str)
    };
    if let Err(e) = result {
        log::error!("操作失败: {}", e);
    }
}

// fn main() {
//     init_logger();
//     let args: Vec<_> = env::args().collect();
//     if args.len() < 2 {
//         println!("用法: {} <连接串> [upload]", args[0]);
//         return;
//     }
//     let conn_str = &args[1];
//     let mode = if args.len() >= 3 && args[2] == "upload" {
//         ProgramMode::UploadThenRead
//     } else {
//         ProgramMode::ReadOnly
//     };
//     if let Err(e) = run_mission_mode(conn_str, mode) {
//         log::error!("运行失败: {}", e);
//     }
// }
// --- 心跳消息（保持不变）---
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
fn read_waypoints(conn_str: &str) -> Result<(), Box<dyn std::error::Error>> {
    // 建立 MAVLink 连接
    let mut conn = mavlink::connect::<MavMessage>(conn_str).expect("连接失败");
    log::info!("✅ 已连接到 {}", conn_str);
    let vehicle = Arc::new(conn);

    // 发送参数请求和数据流请求
    if let Err(e) = vehicle.send_default(&request_parameters()) {
        log::error!("发送参数请求失败: {:?}", e);
    }
    if let Err(e) = vehicle.send_default(&request_stream()) {
        log::error!("发送数据流请求失败: {:?}", e);
    }

    // 心跳发送线程（保持连接活跃）
    let hb_vehicle = vehicle.clone();
    thread::spawn(move || {
        loop {
            if let Err(e) = hb_vehicle.send_default(&heartbeat_message()) {
                log::error!("心跳发送失败: {:?}", e);
            }
            thread::sleep(Duration::from_secs(1));
        }
    });

    // ---------- 新增：计时相关变量 ----------
    let mut start_time: Option<std::time::Instant> = None; // 请求开始时刻
    let mut total_waypoints: u16 = 0; // 记录总航点数（用于最终输出）
    // 状态机初始化
    let mut mission_state = MissionState::Idle;
    let mut start_delay = Duration::from_secs(3); // 延迟3秒后自动请求航点
    let mut start_issued = false;

    log::info!("等待接收消息... (3秒后自动请求航点列表)");

    // 主循环：非阻塞轮询
    loop {
        // 1. 非阻塞尝试接收一条消息
        match vehicle.try_recv() {
            Ok((_header, msg)) => {
                // 2. 根据当前状态机处理消息
                match msg {
                    MavMessage::HEARTBEAT(_) => {
                        log::info!("💓 收到飞控心跳，连接正常");
                    }
                    MavMessage::MISSION_COUNT(cnt) => {
                        log::info!("📊 收到 MISSION_COUNT: total = {}", cnt.count);
                        match mission_state {
                            MissionState::WaitingCount => {
                                let total = cnt.count;
                                total_waypoints = total; // 保存总数
                                if total == 0 {
                                    // 无航点：直接结束计时
                                    if let Some(start) = start_time {
                                        let elapsed = start.elapsed();
                                        log::info!(
                                            "⏱️ 读取 0 个航点耗时: {:.2?}, 平均速度: 0.00 航点/秒",
                                            elapsed
                                        );
                                    }
                                    log::info!("⚠️ 飞控没有航点");
                                    mission_state = MissionState::Idle;
                                } else {
                                    // 开始请求第一个航点
                                    let seq = 0;
                                    let request =
                                        MavMessage::MISSION_REQUEST_INT(MISSION_REQUEST_INT_DATA {
                                            target_system: 1,
                                            target_component: 1,
                                            seq,
                                        });
                                    if let Err(e) = vehicle.send_default(&request) {
                                        log::error!("发送 MISSION_REQUEST_INT 失败: {:?}", e);
                                        mission_state = MissionState::Idle;
                                    } else {
                                        log::info!("📡 已发送 MISSION_REQUEST_INT seq={}", seq);
                                        mission_state = MissionState::Requesting { seq, total };
                                    }
                                }
                            }
                            _ => {
                                // 意外收到 MISSION_COUNT，可能是飞控主动推送，忽略
                                log::info!("⚠️ 未预期的 MISSION_COUNT，忽略");
                            }
                        }
                    }
                    MavMessage::MISSION_ITEM_INT(item) => {
                        log::info!("📍 收到 MISSION_ITEM_INT seq={}", item.seq);
                        match mission_state {
                            MissionState::Requesting { seq, total } => {
                                if item.seq == seq {
                                    // 解码航点 (坐标缩放因子 1e7)
                                    let lat = item.x as f64 / 10_000_000.0;
                                    let lon = item.y as f64 / 10_000_000.0;
                                    let alt = item.z;
                                    log::info!(
                                        "✅ 航点 {}/{}: lat={:.7}, lon={:.7}, alt={:.1} m",
                                        seq + 1,
                                        total,
                                        lat,
                                        lon,
                                        alt
                                    );

                                    let next_seq = seq + 1;
                                    if next_seq < total {
                                        // 请求下一个航点
                                        let request = MavMessage::MISSION_REQUEST_INT(
                                            MISSION_REQUEST_INT_DATA {
                                                target_system: 1,
                                                target_component: 1,
                                                seq: next_seq,
                                            },
                                        );
                                        if let Err(e) = vehicle.send_default(&request) {
                                            log::error!(
                                                "发送下一个 MISSION_REQUEST_INT 失败: {:?}",
                                                e
                                            );
                                            mission_state = MissionState::Idle;
                                        } else {
                                            log::info!(
                                                "📡 已发送 MISSION_REQUEST_INT seq={}",
                                                next_seq
                                            );
                                            mission_state = MissionState::Requesting {
                                                seq: next_seq,
                                                total,
                                            };
                                        }
                                    } else {
                                        // ---------- 所有航点接收完毕，计算耗时 ----------
                                        log::info!("🎉 所有航点读取完成！");
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
                                        mission_state = MissionState::Idle;
                                    }
                                } else {
                                    // 收到的航点序号与期望不符，忽略（可能是重传）
                                    log::info!(
                                        "⚠️ 期望航点 seq={}，收到 seq={}，忽略",
                                        seq,
                                        item.seq
                                    );
                                }
                            }
                            _ => {
                                // 空闲状态收到航点消息，仅打印（可能是飞控主动推送）
                                let lat = item.x as f64 / 10_000_000.0;
                                let lon = item.y as f64 / 10_000_000.0;
                                let alt = item.z;
                                log::info!(
                                    "📍 (主动推送) 航点 {}: lat={:.7}, lon={:.7}, alt={:.1} m",
                                    item.seq,
                                    lat,
                                    lon,
                                    alt
                                );
                            }
                        }
                    }
                    MavMessage::SYSTEM_TIME(msg) => {
                        // 打印系统时间
                        log::info!("🕒 SYSTEM_TIME: {:?}", msg);
                    }
                    MavMessage::PING(msg) => {
                        // 打印 PING 消息
                        log::info!("📨 PING: {:?}", msg);
                    }
                    MavMessage::SYS_STATUS(msg) => {
                        // 打印系统状态
                        log::info!("🛰️ SYS_STATUS: {:?}", msg);
                    }
                    MavMessage::AUTOPILOT_VERSION(msg) => {
                        // 打印固件版本
                        log::info!("📦 AUTOPILOT_VERSION: {:?}", msg);
                    }
                    MavMessage::GLOBAL_POSITION_INT(msg) => {
                        // 打印当前位置
                        log::info!("📍 GLOBAL_POSITION_INT: {:?}", msg);
                    }
                    // POWER_STATUS
                    MavMessage::POWER_STATUS(msg) => {
                        // 打印电源状态
                        log::info!("🔋 POWER_STATUS: {:?}", msg);
                    }
                    // MISSION_CURRENT
                    MavMessage::MISSION_CURRENT(msg) => {
                        // 打印当前航点
                        log::info!("📍 MISSION_CURRENT: {:?}", msg);
                    }
                    // SERVO_OUTPUT_RAW
                    MavMessage::SERVO_OUTPUT_RAW(msg) => {
                        // 打印伺服输出
                        log::info!("🔌 SERVO_OUTPUT_RAW: {:?}", msg);
                    }
                    // RC_CHANNELS
                    MavMessage::RC_CHANNELS(msg) => {
                        // 打印遥控信号
                        log::info!("🎮 RC_CHANNELS: {:?}", msg);
                    }
                    // ATTITUDE
                    MavMessage::ATTITUDE(msg) => {
                        // 打印姿态
                        log::info!("📈 ATTITUDE: {:?}", msg);
                    }
                    // RAW_IMU
                    MavMessage::RAW_IMU(msg) => {
                        // 打印原始 IMU 数据
                        log::info!("📈 RAW_IMU: {:?}", msg);
                    }
                    // SCALED_IMU2
                    MavMessage::SCALED_IMU2(msg) => {
                        // 打印扩展 IMU 数据
                        log::info!("📈 SCALED_IMU2: {:?}", msg);
                    }
                    // SCALED_IMU3
                    MavMessage::SCALED_IMU3(msg) => {
                        // 打印扩展 IMU 数据
                        log::info!("📈 SCALED_IMU3: {:?}", msg);
                    }
                    // RC_CHANNELS_SCALED
                    MavMessage::RC_CHANNELS_SCALED(msg) => {
                        // 打印扩展遥控信号
                        log::info!("🎮 RC_CHANNELS_SCALED: {:?}", msg);
                    }
                    // SCALED_PRESSURE
                    MavMessage::SCALED_PRESSURE(msg) => {
                        // 打印扩展压力传感器数据
                        log::info!("📈 SCALED_PRESSURE: {:?}", msg);
                    }
                    // GPS_RAW_INT
                    MavMessage::GPS_RAW_INT(msg) => {
                        // 打印 GPS 数据
                        log::info!("📡 GPS_RAW_INT: {:?}", msg);
                    }
                    // EKF_STATUS_REPORT
                    MavMessage::EKF_STATUS_REPORT(msg) => {
                        // 打印 EKF 状态
                        log::info!("📈 EKF_STATUS_REPORT: {:?}", msg);
                    }
                    // VIBRATION
                    MavMessage::VIBRATION(msg) => {
                        // 打印振动数据
                        log::info!("📈 VIBRATION: {:?}", msg);
                    }
                    // BATTERY_STATUS
                    MavMessage::BATTERY_STATUS(msg) => {
                        // 打印电池状态
                        log::info!("🔋 BATTERY_STATUS: {:?}", msg);
                    }
                    // COMMAND_ACK
                    MavMessage::COMMAND_ACK(msg) => {
                        // 打印命令确认
                        log::info!("📨 COMMAND_ACK: {:?}", msg);
                    }
                    // MEMINFO
                    MavMessage::MEMINFO(msg) => {
                        // 打印内存信息
                        log::info!("📨 MEMINFO: {:?}", msg);
                    }
                    // VFR_HUD
                    MavMessage::VFR_HUD(msg) => {
                        // 打印飞行状态
                        log::info!("📈 VFR_HUD: {:?}", msg);
                    }
                    // AHRS
                    MavMessage::AHRS(msg) => {
                        // 打印 AHRS 数据
                        log::info!("📈 AHRS: {:?}", msg);
                    }
                    // AHRS2
                    MavMessage::AHRS2(msg) => {
                        // 打印 AHRS 数据
                        log::info!("📈 AHRS2: {:?}", msg);
                    }
                    // PARAM_VALUE
                    MavMessage::PARAM_VALUE(msg) => {
                        // 打印参数值
                        log::info!("📨 PARAM_VALUE: {:?}", msg);
                    }
                    _ => {
                        // 其他消息不处理，保持状态机不变
                        // 如果需要调试，可以取消注释：
                        log::info!("收到其他消息: {:?}", msg);
                    }
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

        // 3. 定时触发航点列表请求（仅触发一次）
        // if !start_issued {
        //     if start_delay <= Duration::ZERO {
        //         // ---------- 记录开始时间 ----------
        //         start_time = Some(std::time::Instant::now());
        //         let request_list = MavMessage::MISSION_REQUEST_LIST(MISSION_REQUEST_LIST_DATA {
        //             target_system: 1,
        //             target_component: 1,
        //         });
        //         if let Err(e) = vehicle.send_default(&request_list) {
        //             log::error!("发送 MISSION_REQUEST_LIST 失败: {:?}", e);
        //         } else {
        //             log::info!("📡 已发送 MISSION_REQUEST_LIST，等待 MISSION_COUNT...");
        //             mission_state = MissionState::WaitingCount;
        //         }
        //         start_issued = true;
        //     } else {
        //         start_delay -= Duration::from_millis(10);
        //     }
        // }
    }
    unimplemented!("请将原有 main 函数的读取逻辑移动到 read_waypoints 中")
}

// -----------------------------------------------------------------------------
// 上传随机航点（覆盖飞控现有航线）
// -----------------------------------------------------------------------------
fn upload_random_waypoints(conn_str: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = mavlink::connect::<MavMessage>(conn_str)?;
    log::info!("✅ 已连接到 {}，准备上传随机航点", conn_str);

    // 建立 MAVLink 连接
    let vehicle = Arc::new(conn);

    // 发送参数请求和数据流请求
    if let Err(e) = vehicle.send_default(&request_parameters()) {
        log::error!("发送参数请求失败: {:?}", e);
    }
    if let Err(e) = vehicle.send_default(&request_stream()) {
        log::error!("发送数据流请求失败: {:?}", e);
    }

    // 心跳发送线程（保持连接活跃）
    let hb_vehicle = vehicle.clone();
    thread::spawn(move || {
        loop {
            if let Err(e) = hb_vehicle.send_default(&heartbeat_message()) {
                log::error!("心跳发送失败: {:?}", e);
            }
            thread::sleep(Duration::from_secs(1));
        }
    });

    // 等待飞控稳定
    // thread::sleep(Duration::from_secs(2));
    let mut mission_state = MissionState::Idle;
    let mut start_delay = Duration::from_secs(3); // 延迟3秒后自动请求航点
    let mut start_issued = false;
    // ---------- 3. 生成随机航点 ----------
    let mut rng = rand::rng();
    // let waypoint_count = rng.random_range(300..=600);
    let waypoint_count = 1000;
    log::info!("🎲 将上传 {} 个随机航点", waypoint_count);
    let mut start_time: Option<std::time::Instant> = None; // 请求开始时刻

    // 主循环：非阻塞轮询
    loop {
        // 1. 非阻塞尝试接收一条消息
        match vehicle.try_recv() {
            Ok((_header, msg)) => {
                // 2. 根据当前状态机处理消息
                match msg {
                    MavMessage::HEARTBEAT(_) => {
                        log::info!("💓 收到飞控心跳，连接正常");
                    }
                    MavMessage::MISSION_ACK(ack) => {
                        if let Some(start) = start_time {
                            let elapsed = start.elapsed();
                            let rate = waypoint_count as f64 / elapsed.as_secs_f64();
                            log::info!(
                                "⏱️ 读取 {} 个航点耗时: {:.2?}, 平均速度: {:.2} 航点/秒",
                                waypoint_count,
                                elapsed,
                                rate
                            );
                        }
                        if ack.mavtype == MavMissionResult::MAV_MISSION_ACCEPTED {
                            log::info!("✅ 飞控确认接收，航线上传成功！");
                            return Ok(());
                        } else {
                            log::error!("❌ 飞控拒绝航线：{:?}", ack.mavtype);
                            return Err("Mission upload rejected".into());
                        }
                    }
                    MavMessage::MISSION_COUNT(cnt) => {
                        log::info!("📊 收到 MISSION_COUNT: total = {}", cnt.count);
                    }
                    MavMessage::MISSION_ITEM_INT(item) => {
                        log::info!("📍 收到 MISSION_ITEM_INT seq={}", item.seq);
                    }
                    _ => {
                        // 其他消息不处理，保持状态机不变
                        // 如果需要调试，可以取消注释：
                        log::info!("收到其他消息: {:?}", msg);
                    }
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

        // 3. 定时触发航点列表请求（仅触发一次）
        if !start_issued {
            if start_delay <= Duration::ZERO {
                // ---------- 记录开始时间 ----------
                start_time = Some(std::time::Instant::now());
                let mut waypoints = Vec::new();
                for i in 0..waypoint_count {
                    let lat = 30.0 + rng.random_range(0.0..1.0);
                    let lon = 120.0 + rng.random_range(0.0..1.0);
                    let alt = rng.random_range(50.0..200.0);
                    waypoints.push((lat, lon, alt));
                    log::info!(
                        "生成航点 {}: lat={:.7}, lon={:.7}, alt={:.1}m",
                        i + 1,
                        lat,
                        lon,
                        alt
                    );
                }

                // ---------- 4. 发送 MISSION_COUNT（必须包含 mission_type）----------
                let count_msg = MavMessage::MISSION_COUNT(MISSION_COUNT_DATA {
                    target_system: 1,
                    target_component: 1,
                    count: waypoint_count as u16,
                    // mission_type: 0, // 标准任务，不能省略
                });
                vehicle.send_default(&count_msg)?;
                log::info!("📤 已发送 MISSION_COUNT (count={})", waypoint_count);
                // 短暂延时，让飞控准备接收
                std::thread::sleep(Duration::from_millis(500));

                // 直接发送所有航点（不等待请求，模仿Python成功案例）
                for seq in 0..waypoint_count as u16 {
                    let (lat, lon, alt) = waypoints[seq as usize];
                    let x = (lat * 10_000_000.0) as i32;
                    let y = (lon * 10_000_000.0) as i32;
                    let z = alt as f32;

                    let item = MavMessage::MISSION_ITEM_INT(MISSION_ITEM_INT_DATA {
                        target_system: 1,
                        target_component: 1,
                        seq,
                        frame: MavFrame::MAV_FRAME_GLOBAL_RELATIVE_ALT,
                        command: MavCmd::MAV_CMD_NAV_WAYPOINT,
                        current: if seq == 0 { 1 } else { 0 },
                        autocontinue: 1,
                        param1: 0.0,
                        param2: 0.0,
                        param3: 0.0,
                        param4: 0.0,
                        x,
                        y,
                        z,
                        // mission_type: 0,
                    });
                    vehicle.send_default(&item)?;
                    log::info!("📤 已发送航点 {} / {}", seq + 1, waypoint_count);
                    // 稍微延时，避免消息拥塞
                    std::thread::sleep(Duration::from_millis(20));
                }

                // if let Err(e) = vehicle.send_default(&request_list) {
                //     log::error!("发送 MISSION_REQUEST_LIST 失败: {:?}", e);
                // } else {
                //     log::info!("📡 已发送 MISSION_REQUEST_LIST，等待 MISSION_COUNT...");
                //     mission_state = MissionState::WaitingCount;
                // }
                start_issued = true;
            } else {
                start_delay -= Duration::from_millis(10);
            }
        }
    }

    Ok(())
}

// fn run_mission_mode(conn_str: &str, mode: ProgramMode) -> Result<(), Box<dyn std::error::Error>> {
//     let mut conn = mavlink::connect::<MavMessage>(conn_str)?;
//     log::info!("✅ 已连接到 {}，准备上传随机航点", conn_str);

//     // 建立 MAVLink 连接
//     let vehicle = Arc::new(Mutex::new(conn));

//     // 发送参数请求和数据流请求
//     if let Err(e) = vehicle.send_default(&request_parameters()) {
//         log::error!("发送参数请求失败: {:?}", e);
//     }
//     if let Err(e) = vehicle.send_default(&request_stream()) {
//         log::error!("发送数据流请求失败: {:?}", e);
//     }

//     // 心跳发送线程（保持连接活跃）
//     let hb_vehicle = vehicle.clone();
//     thread::spawn(move || {
//         loop {
//             if let Err(e) = hb_vehicle.send_default(&heartbeat_message()) {
//                 log::error!("心跳发送失败: {:?}", e);
//             }
//             thread::sleep(Duration::from_secs(1));
//         }
//     });

//     let mut mission_state = MissionState::Idle;
//     let mut start_delay = Duration::from_secs(3);
//     let mut start_issued = false;

//     // 上传相关变量
//     let mut waypoints: Vec<(f64, f64, f64)> = Vec::new();
//     let mut total_waypoints = 0;
//     let mut next_seq_to_send = 0;

//     // 读取模式相关变量
//     let mut read_total = 0;
//     let mut read_seq = 0;
//     let mut start_time: Option<std::time::Instant> = None;

//     log::info!("等待接收消息... (3秒后自动开始)");
//     let mut rng = rand::rng();
//     let waypoint_count = rng.random_range(300..=600);
//     log::info!("🎲 将上传 {} 个随机航点", waypoint_count);
//     // 定时触发任务（读取或上传）
//     match mode {
//         ProgramMode::ReadOnly => {
//             // 发送 MISSION_REQUEST_LIST 读取航点
//             let req_list = MavMessage::MISSION_REQUEST_LIST(MISSION_REQUEST_LIST_DATA {
//                 target_system: 1,
//                 target_component: 1,
//             });
//             vehicle.send_default(&req_list)?;
//             log::info!("📡 已发送 MISSION_REQUEST_LIST，等待 MISSION_COUNT...");
//             mission_state = MissionState::WaitingCount;
//             start_time = Some(std::time::Instant::now());
//         }
//         ProgramMode::UploadThenRead => {
//             // start_time = Some(std::time::Instant::now());
//             let mut waypoints = Vec::new();
//             for i in 0..waypoint_count {
//                 let lat = 30.0 + rng.random_range(0.0..1.0);
//                 let lon = 120.0 + rng.random_range(0.0..1.0);
//                 let alt = rng.random_range(50.0..200.0);
//                 waypoints.push((lat, lon, alt));
//                 log::info!(
//                     "生成航点 {}: lat={:.7}, lon={:.7}, alt={:.1}m",
//                     i + 1,
//                     lat,
//                     lon,
//                     alt
//                 );
//             }

//             // ---------- 4. 发送 MISSION_COUNT（必须包含 mission_type）----------
//             let count_msg = MavMessage::MISSION_COUNT(MISSION_COUNT_DATA {
//                 target_system: 1,
//                 target_component: 1,
//                 count: waypoint_count as u16,
//                 // mission_type: 0, // 标准任务，不能省略
//             });
//             vehicle.send_default(&count_msg)?;
//             log::info!("📤 已发送 MISSION_COUNT (count={})", waypoint_count);
//             // 短暂延时，让飞控准备接收
//             std::thread::sleep(Duration::from_millis(500));

//             // 直接发送所有航点（不等待请求，模仿Python成功案例）
//             for seq in 0..waypoint_count as u16 {
//                 let (lat, lon, alt) = waypoints[seq as usize];
//                 let x = (lat * 10_000_000.0) as i32;
//                 let y = (lon * 10_000_000.0) as i32;
//                 let z = alt as f32;

//                 let item = MavMessage::MISSION_ITEM_INT(MISSION_ITEM_INT_DATA {
//                     target_system: 1,
//                     target_component: 1,
//                     seq,
//                     frame: MavFrame::MAV_FRAME_GLOBAL_RELATIVE_ALT,
//                     command: MavCmd::MAV_CMD_NAV_WAYPOINT,
//                     current: if seq == 0 { 1 } else { 0 },
//                     autocontinue: 1,
//                     param1: 0.0,
//                     param2: 0.0,
//                     param3: 0.0,
//                     param4: 0.0,
//                     x,
//                     y,
//                     z,
//                     // mission_type: 0,
//                 });
//                 vehicle.send_default(&item)?;
//                 log::info!("📤 已发送航点 {} / {}", seq + 1, waypoint_count);
//                 // 稍微延时，避免消息拥塞
//                 std::thread::sleep(Duration::from_millis(20));
//             }

//             // 上传完成后，重置状态机，准备切换到读取模式
//             start_issued = false;
//             start_delay = Duration::from_secs(3);
//             mission_state = MissionState::Idle;
//         }
//     }
//     loop {
//         // 非阻塞接收
//         match vehicle.try_recv() {
//             Ok((_header, msg)) => match msg {
//                 MavMessage::HEARTBEAT(_) => {
//                     log::info!("💓 收到飞控心跳");
//                 }
//                 MavMessage::MISSION_COUNT(cnt) => {
//                     log::info!("📊 收到 MISSION_COUNT: total = {}", cnt.count);
//                     if mission_state == MissionState::WaitingCount {
//                         // 读取模式：开始请求航点
//                         let total = cnt.count;
//                         read_total = total;
//                         if total == 0 {
//                             log::info!("⚠️ 飞控没有航点");
//                             mission_state = MissionState::Idle;
//                         } else {
//                             // 请求第一个航点
//                             let req = MavMessage::MISSION_REQUEST_INT(MISSION_REQUEST_INT_DATA {
//                                 target_system: 1,
//                                 target_component: 1,
//                                 seq: 0,
//                             });
//                             vehicle.send_default(&req)?;
//                             log::info!("📡 已发送 MISSION_REQUEST_INT seq=0");
//                         }
//                     }
//                 }
//                 // MavMessage::MISSION_REQUEST_INT(req) => {
//                 //     log::info!("📥 收到 MISSION_REQUEST_INT seq={}", req.seq);
//                 //     if mission_state == MissionState::Uploading {
//                 //         // 上传模式：根据请求发送对应的航点
//                 //         if req.seq == next_seq_to_send && next_seq_to_send < total_waypoints {
//                 //             let (lat, lon, alt) = waypoints[next_seq_to_send as usize];
//                 //             let x = (lat * 10_000_000.0) as i32;
//                 //             let y = (lon * 10_000_000.0) as i32;
//                 //             let z = alt as f32;
//                 //             let item = MavMessage::MISSION_ITEM_INT(MISSION_ITEM_INT_DATA {
//                 //                 target_system: 1,
//                 //                 target_component: 1,
//                 //                 seq: next_seq_to_send,
//                 //                 frame: MavFrame::MAV_FRAME_GLOBAL_RELATIVE_ALT,
//                 //                 command: MavCmd::MAV_CMD_NAV_WAYPOINT,
//                 //                 current: if next_seq_to_send == 0 { 1 } else { 0 },
//                 //                 autocontinue: 1,
//                 //                 param1: 0.0,
//                 //                 param2: 0.0,
//                 //                 param3: 0.0,
//                 //                 param4: 0.0,
//                 //                 x,
//                 //                 y,
//                 //                 z,
//                 //                 // mission_type: 0,
//                 //             });
//                 //             vehicle.send_default(&item)?;
//                 //             log::info!(
//                 //                 "📤 已发送航点 {} / {}",
//                 //                 next_seq_to_send + 1,
//                 //                 total_waypoints
//                 //             );
//                 //             next_seq_to_send += 1;
//                 //             if next_seq_to_send == total_waypoints {
//                 //                 log::info!("所有航点已发送，等待 MISSION_ACK...");
//                 //             }
//                 //         } else {
//                 //             log::warn!("意外的请求 seq={}, 期望 {}", req.seq, next_seq_to_send);
//                 //         }
//                 //     } else if mission_state == MissionState::WaitingCount {
//                 //         // 读取模式：收到请求后发送航点
//                 //         if req.seq == read_seq && read_seq < read_total {
//                 //             // 这里本应发送 MISSION_ITEM_INT，但我们是读取端，不应该发送航点
//                 //             // 实际上在读取模式中，飞控应该发送 MISSION_ITEM_INT，而不是请求。所以这个分支一般不会进入。
//                 //             log::warn!("读取模式中收到了 MISSION_REQUEST_INT，可能协议异常");
//                 //         }
//                 //     }
//                 // }
//                 MavMessage::MISSION_ITEM_INT(item) => {
//                     log::info!("📍 收到 MISSION_ITEM_INT seq={}", item.seq);
//                     if mission_state == MissionState::WaitingCount {
//                         // 读取模式：接收航点
//                         if item.seq == read_seq {
//                             let lat = item.x as f64 / 10_000_000.0;
//                             let lon = item.y as f64 / 10_000_000.0;
//                             let alt = item.z;
//                             log::info!(
//                                 "✅ 航点 {}/{}: lat={:.7}, lon={:.7}, alt={:.1}m",
//                                 read_seq + 1,
//                                 read_total,
//                                 lat,
//                                 lon,
//                                 alt
//                             );
//                             read_seq += 1;
//                             if read_seq < read_total {
//                                 // 请求下一个航点
//                                 let req =
//                                     MavMessage::MISSION_REQUEST_INT(MISSION_REQUEST_INT_DATA {
//                                         target_system: 1,
//                                         target_component: 1,
//                                         seq: read_seq,
//                                     });
//                                 vehicle.send_default(&req)?;
//                                 log::info!("📡 已发送 MISSION_REQUEST_INT seq={}", read_seq);
//                             } else {
//                                 log::info!("🎉 所有航点读取完成！");
//                                 if let Some(start) = start_time {
//                                     let elapsed = start.elapsed();
//                                     let rate = read_total as f64 / elapsed.as_secs_f64();
//                                     log::info!(
//                                         "⏱️ 读取 {} 个航点耗时: {:.2?}, 平均速度: {:.2} 航点/秒",
//                                         read_total,
//                                         elapsed,
//                                         rate
//                                     );
//                                 }
//                                 mission_state = MissionState::Idle;
//                             }
//                         } else {
//                             log::warn!("期望 seq={}，收到 seq={}，忽略", read_seq, item.seq);
//                         }
//                     }
//                 }
//                 MavMessage::MISSION_ACK(ack) => {
//                     if ack.mavtype == MavMissionResult::MAV_MISSION_ACCEPTED {
//                         log::info!("✅ 飞控确认接收，航线上传成功！");
//                         if let ProgramMode::UploadThenRead = mode {
//                             log::info!("上传完成，3秒后切换到读取模式...");
//                             // 重置状态机，准备读取
//                             mission_state = MissionState::Idle;
//                             start_issued = false;
//                             start_delay = Duration::from_secs(3);
//                             waypoints.clear();
//                             total_waypoints = 0;
//                             next_seq_to_send = 0;
//                         } else {
//                             log::info!("上传成功，程序继续运行...");
//                         }
//                     } else {
//                         log::error!("❌ 飞控拒绝航线: {:?}", ack.mavtype);
//                     }
//                 }
//                 _ => {
//                     // 其他消息，按需打印
//                     // log::debug!("收到其他消息: {:?}", msg);
//                 }
//             },
//             Err(MessageReadError::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
//                 thread::sleep(Duration::from_millis(10));
//             }
//             Err(e) => {
//                 log::error!("接收错误: {:?}", e);
//                 // break;
//             }
//         }
//     }

//     Ok(())
// }
