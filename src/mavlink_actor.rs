use anyhow::Result;
use mavlink::ardupilotmega::MavCmd::{MAV_CMD_COMPONENT_ARM_DISARM, MAV_CMD_DO_SET_MODE};
use mavlink::ardupilotmega::{
    COMMAND_LONG_DATA, MISSION_ITEM_INT_DATA, MISSION_REQUEST_INT_DATA, MISSION_REQUEST_LIST_DATA,
    MavMessage::REQUEST_DATA_STREAM, REQUEST_DATA_STREAM_DATA, RoverMode,
};
use mavlink::ardupilotmega::{MISSION_CLEAR_ALL_DATA, MISSION_COUNT_DATA};
use std::sync::Arc;

use mavlink::{MavConnection, MavHeader, ardupilotmega::MavMessage};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

// 默认飞控地址。在收到可信 HEARTBEAT 后，后续可由动态发现的地址覆盖。
pub const DEFAULT_TARGET_SYSTEM_ID: u8 = 1;
pub const DEFAULT_TARGET_COMPONENT_ID: u8 = 1;

// 定义航点结构体，用于序列化响应
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Waypoint {
    pub seq: u16,
    pub lat: f64,
    pub lon: f64,
    pub alt: f32,
}
// 写入的结构体
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WaypointWrite {
    pub seq: u16,
}
#[derive(Debug)]
pub enum MavlinkActorMessage {
    // MAVLink 消息
    MavlinkMessage((MavHeader, MavMessage)),
    RequestWaypointList, // 请求航点列表
    ArmDisarm { arm: bool },
    SetMode { mode: String },                     // 设置飞行模式
    SetWaypointList { waypoints: Vec<Waypoint> }, // 设置航点列表
    ClearWaypointList,                            // 清除航点列表
}
#[derive(Debug, Serialize, Deserialize, Clone)]

pub enum MissionState {
    Idle,
    Clearing,
    WaitingCount,
    Downloading {
        expected_count: u16,
        next_seq: u16,
        received: Vec<Waypoint>,
    },
    Uploading {
        waypoints: Vec<Waypoint>,
        current_index: usize,
    },
}
impl Default for MissionState {
    fn default() -> Self {
        MissionState::Idle
    }
}
// 定义 MavlinkActor
pub struct MavlinkActor {
    vehicle: Arc<Box<dyn MavConnection<MavMessage> + Send + Sync>>,
    state: MissionState,
    pending_download: bool,
}

impl MavlinkActor {
    pub fn new(vehicle: Arc<Box<dyn MavConnection<MavMessage> + Send + Sync>>) -> Self {
        Self {
            vehicle,
            state: MissionState::Idle,
            pending_download: false,
        }
    }

    pub async fn run(mut self, mut rx: mpsc::Receiver<MavlinkActorMessage>) -> Result<()> {
        loop {
            tokio::select! {
                // 处理 Actor 消息
                Some(msg) = rx.recv() => {
                    match msg {
                     MavlinkActorMessage::MavlinkMessage((header, msg)) => {
                            // 处理 MAVLink 消息
                            self.handle_mavlink_message(header, msg)?;
                        }
                        MavlinkActorMessage::RequestWaypointList => {
                            // 处理航点列表请求
                            self.handle_request_waypoint_list()?;
                        }
                        MavlinkActorMessage::ArmDisarm { arm } => {
                            self.handle_arm_disarm(arm)?;
                        }
                        MavlinkActorMessage::SetMode { mode } => {
                            self.handle_set_mode(mode)?;
                        }
                        MavlinkActorMessage::SetWaypointList { waypoints } => {
                            self.handle_set_waypoint_list(waypoints)?;
                        }
                        MavlinkActorMessage::ClearWaypointList => {
                            self.handle_clear_waypoint_list()?;
                        }
                    }
                }
                else => break,
            }
        }
        Ok(())
    }

    fn handle_mavlink_message(&mut self, _header: MavHeader, msg: MavMessage) -> Result<()> {
        // 根据消息类型处理状态转换
        match msg {
            MavMessage::MISSION_COUNT(cnt) => {
                log::info!("航点数量:{cnt:?}");
                self.state = match self.state {
                    MissionState::WaitingCount => {
                        if cnt.count == 0 {
                            MissionState::Idle
                        } else {
                            // 请求第一个航点
                            let req = MavMessage::MISSION_REQUEST_INT(MISSION_REQUEST_INT_DATA {
                                target_system: DEFAULT_TARGET_SYSTEM_ID,
                                target_component: DEFAULT_TARGET_COMPONENT_ID,
                                seq: 0,
                            });
                            if let Err(e) = self.vehicle.send_default(&req) {
                                log::error!("发送 MISSION_REQUEST_INT 失败: {}", e);
                                MissionState::Idle
                            } else {
                                MissionState::Downloading {
                                    expected_count: cnt.count,
                                    next_seq: 0,
                                    received: Vec::new(),
                                }
                            }
                        }
                    }
                    _ => self.state.clone(),
                };
            }
            MavMessage::MISSION_ITEM_INT(item) => {
                log::info!("航点信息:{item:?}");
                self.state = match std::mem::take(&mut self.state) {
                    MissionState::Downloading {
                        expected_count,
                        next_seq,
                        mut received,
                    } if item.seq == next_seq && item.seq < expected_count => {
                        let wp = Waypoint {
                            seq: item.seq,
                            lat: item.x as f64 / 1e7,
                            lon: item.y as f64 / 1e7,
                            alt: item.z,
                        };
                        received.push(wp);

                        let following_seq = next_seq.saturating_add(1);
                        if following_seq < expected_count {
                            // 请求下一个航点
                            let req = MavMessage::MISSION_REQUEST_INT(MISSION_REQUEST_INT_DATA {
                                target_system: DEFAULT_TARGET_SYSTEM_ID,
                                target_component: DEFAULT_TARGET_COMPONENT_ID,
                                seq: following_seq,
                            });
                            if let Err(e) = self.vehicle.send_default(&req) {
                                log::error!("发送下一个 MISSION_REQUEST_INT 失败: {}", e);
                                MissionState::Idle
                            } else {
                                MissionState::Downloading {
                                    expected_count,
                                    next_seq: following_seq,
                                    received,
                                }
                            }
                        } else {
                            // 所有航点接收完毕
                            MissionState::Idle
                        }
                    }
                    MissionState::Downloading {
                        expected_count,
                        next_seq,
                        received,
                    } => {
                        log::warn!(
                            "收到非预期航点，seq={}，期望={}，总数={}",
                            item.seq,
                            next_seq,
                            expected_count
                        );
                        MissionState::Downloading {
                            expected_count,
                            next_seq,
                            received,
                        }
                    }
                    _ => self.state.clone(),
                }
            }
            // HEARTBEAT
            MavMessage::HEARTBEAT(heartbeat) => {
                log::info!("收到心跳: {heartbeat:?}");
            }
            // 新版飞控通常使用 MISSION_REQUEST_INT；保留旧消息以兼容老设备。
            MavMessage::MISSION_REQUEST_INT(req) => {
                log::info!("收到 MISSION_REQUEST_INT 航点请求: {req:?}");
                self.handle_mission_request(req.seq)?;
            }
            #[allow(deprecated)]
            MavMessage::MISSION_REQUEST(req) => {
                log::info!("收到旧版 MISSION_REQUEST 航点请求: {req:?}");
                self.handle_mission_request(req.seq)?;
            }
            // 加解锁
            MavMessage::COMMAND_ACK(_ack) => {
                // log::info!("收到解锁/上锁响应: {ack:?}");
            }
            MavMessage::MISSION_ACK(_ack) => match &self.state {
                MissionState::Clearing => {
                    self.state = MissionState::Idle;
                    log::info!("清除航线完成");
                }
                MissionState::Uploading {
                    waypoints,
                    current_index,
                } if *current_index == waypoints.len() => {
                    if self.pending_download {
                        self.pending_download = false;
                        self.send_mission_request_list()?;
                    } else {
                        self.state = MissionState::Idle;
                    }
                }
                MissionState::Uploading { .. } => {
                    log::debug!("忽略航点上传完成前收到的 MISSION_ACK");
                }
                _ => {}
            },
            _ => {}
        }

        Ok(())
    }

    fn handle_mission_request(&mut self, seq: u16) -> Result<()> {
        self.state = match std::mem::take(&mut self.state) {
            MissionState::Uploading { waypoints, .. } if seq < waypoints.len() as u16 => {
                let wp = &waypoints[seq as usize];
                let item = MavMessage::MISSION_ITEM_INT(MISSION_ITEM_INT_DATA {
                    target_system: DEFAULT_TARGET_SYSTEM_ID,
                    target_component: DEFAULT_TARGET_COMPONENT_ID,
                    // 飞控请求的 seq 才是协议中的航点序号；输入数据的 seq 可能不连续。
                    seq,
                    frame: mavlink::ardupilotmega::MavFrame::MAV_FRAME_GLOBAL_RELATIVE_ALT,
                    command: mavlink::ardupilotmega::MavCmd::MAV_CMD_NAV_WAYPOINT,
                    current: if seq == 0 { 1 } else { 0 },
                    autocontinue: 1,
                    param1: 0.0,
                    param2: 0.0,
                    param3: 0.0,
                    param4: 0.0,
                    x: (wp.lat * 1e7) as i32,
                    y: (wp.lon * 1e7) as i32,
                    z: wp.alt,
                });
                if let Err(e) = self.vehicle.send_default(&item) {
                    log::error!("发送航点失败: {}", e);
                    MissionState::Idle
                } else {
                    MissionState::Uploading {
                        waypoints,
                        current_index: seq as usize + 1,
                    }
                }
            }
            MissionState::Uploading { .. } => {
                log::warn!("收到超出范围的航点请求，seq={seq}");
                MissionState::Idle
            }
            other => other,
        };
        Ok(())
    }

    fn handle_set_waypoint_list(&mut self, waypoints: Vec<Waypoint>) -> Result<()> {
        log::info!("收到设置航点列表请求，共 {} 个航点", waypoints.len());
        if !matches!(self.state, MissionState::Idle) {
            log::warn!("任务操作正在进行，拒绝并发写入航线请求");
            return Ok(());
        }
        if waypoints.len() > u16::MAX as usize {
            log::error!("航点数量超过 MAVLink 限制: {}", waypoints.len());
            return Ok(());
        }
        self.pending_download = false;
        if waypoints.is_empty() {
            return self.handle_clear_waypoint_list();
        }

        // MISSION_COUNT 会替换飞控上的任务列表，无需先清除旧航线。
        // 这样也不会把清除操作的 ACK 误判成上传完成。
        self.state = MissionState::Uploading {
            waypoints: waypoints.clone(),
            current_index: 0,
        };
        self.send_mission_count(&waypoints)
    }

    fn handle_clear_waypoint_list(&mut self) -> Result<()> {
        if !matches!(self.state, MissionState::Idle) {
            log::warn!("任务操作正在进行，拒绝清除航线请求");
            return Ok(());
        }

        self.pending_download = false;
        let clear_msg = MavMessage::MISSION_CLEAR_ALL(MISSION_CLEAR_ALL_DATA {
            target_system: DEFAULT_TARGET_SYSTEM_ID,
            target_component: DEFAULT_TARGET_COMPONENT_ID,
        });
        if let Err(e) = self.vehicle.send_default(&clear_msg) {
            log::error!("发送清除航点命令失败: {}", e);
        } else {
            self.state = MissionState::Clearing;
        }
        Ok(())
    }

    fn send_mission_count(&mut self, waypoints: &[Waypoint]) -> Result<()> {
        let count_msg = MavMessage::MISSION_COUNT(MISSION_COUNT_DATA {
            target_system: DEFAULT_TARGET_SYSTEM_ID,
            target_component: DEFAULT_TARGET_COMPONENT_ID,
            count: waypoints.len() as u16,
        });
        if let Err(e) = self.vehicle.send_default(&count_msg) {
            log::error!("发送航点数量失败: {}", e);
            self.state = MissionState::Idle;
        }
        Ok(())
    }

    fn handle_request_waypoint_list(&mut self) -> Result<()> {
        log::info!("收到航点列表请求命令");
        match self.state {
            MissionState::Idle => self.send_mission_request_list(),
            MissionState::Uploading { .. } => {
                self.pending_download = true;
                log::warn!("当前正在写入航线，读取请求已排队等待上传完成");
                Ok(())
            }
            MissionState::Clearing
            | MissionState::WaitingCount
            | MissionState::Downloading { .. } => {
                log::warn!("当前正在读取航线，忽略重复读取请求");
                Ok(())
            }
        }
    }

    fn send_mission_request_list(&mut self) -> Result<()> {
        let req = MavMessage::MISSION_REQUEST_LIST(MISSION_REQUEST_LIST_DATA {
            target_system: DEFAULT_TARGET_SYSTEM_ID,
            target_component: DEFAULT_TARGET_COMPONENT_ID,
        });
        if let Err(e) = self.vehicle.send_default(&req) {
            log::error!("发送 MISSION_REQUEST_LIST 失败: {}", e);
            self.state = MissionState::Idle;
        } else {
            self.state = MissionState::WaitingCount;
        }
        Ok(())
    }

    fn handle_arm_disarm(&mut self, arm: bool) -> Result<()> {
        log::info!("收到解锁/上锁命令: {}", arm);
        let msg = MavMessage::COMMAND_LONG(COMMAND_LONG_DATA {
            target_system: DEFAULT_TARGET_SYSTEM_ID,
            target_component: DEFAULT_TARGET_COMPONENT_ID,
            command: MAV_CMD_COMPONENT_ARM_DISARM, // 400
            confirmation: 1,                       // 0=首次发送，1=确认
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
    fn handle_set_mode(&mut self, mode: String) -> Result<()> {
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
            target_system: DEFAULT_TARGET_SYSTEM_ID,
            target_component: DEFAULT_TARGET_COMPONENT_ID,
            command: MAV_CMD_DO_SET_MODE, // MAV_CMD_DO_SET_MODE 的命令编号
            confirmation: 0,
            param1: 1.0,               // 固定为1.0
            param2: mode_value as f32, // 传入的模式编号
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
            target_system: DEFAULT_TARGET_SYSTEM_ID,
            target_component: DEFAULT_TARGET_COMPONENT_ID,
        },
    )
}

pub fn request_stream() -> mavlink::ardupilotmega::MavMessage {
    #[expect(deprecated)]
    REQUEST_DATA_STREAM(REQUEST_DATA_STREAM_DATA {
        target_system: DEFAULT_TARGET_SYSTEM_ID,
        target_component: DEFAULT_TARGET_COMPONENT_ID,
        req_stream_id: 0,
        req_message_rate: 10,
        start_stop: 1,
    })
}
