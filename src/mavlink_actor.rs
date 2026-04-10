use anyhow::Result;
use mavlink::ardupilotmega::MavCmd::MAV_CMD_COMPONENT_ARM_DISARM;
use mavlink::ardupilotmega::{
    COMMAND_LONG_DATA, MISSION_REQUEST_INT_DATA, MISSION_REQUEST_LIST_DATA,
};
use std::sync::Arc;

use mavlink::{MavConnection, MavHeader, ardupilotmega::MavMessage};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

// 定义航点结构体，用于序列化响应
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Waypoint {
    pub seq: u16,
    pub lat: f64,
    pub lon: f64,
    pub alt: f32,
}
#[derive(Debug)]
pub enum MavlinkActorMessage {
    // MAVLink 消息
    MavlinkMessage((MavHeader, MavMessage)),
    RequestWaypointList,  // 请求航点列表
    RequestWaypoint(u16), // 请求指定序号的航点
    ArmDisarm { arm: bool },
}
#[derive(Debug, Serialize, Deserialize, Clone)]

pub enum MissionState {
    Idle,
    WaitingCount,
    Downloading {
        expected_count: u16,
        received: Vec<Waypoint>,
    },
}
// 定义 MavlinkActor
pub struct MavlinkActor {
    vehicle: Arc<Box<dyn MavConnection<MavMessage> + Send + Sync>>,
    state: MissionState,
    tx: mpsc::Sender<(MavHeader, MavMessage)>,
}

impl MavlinkActor {
    pub fn new(
        vehicle: Arc<Box<dyn MavConnection<MavMessage> + Send + Sync>>,
        tx: mpsc::Sender<(MavHeader, MavMessage)>,
    ) -> Self {
        Self {
            vehicle,
            state: MissionState::Idle,
            tx,
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
                            self.handle_mavlink_message(header, msg).await?;
                        }
                        MavlinkActorMessage::RequestWaypointList => {
                            // 处理航点列表请求
                            self.handle_request_waypoint_list().await?;
                        }
                        MavlinkActorMessage::RequestWaypoint(seq) => {
                            // 处理航点请求
                            self.handle_request_waypoint(seq).await?;
                        }
                        MavlinkActorMessage::ArmDisarm { arm } => {
                            self.handle_arm_disarm(arm).await?;
                        }
                    }
                }
            }
        }
    }

    async fn handle_mavlink_message(&mut self, _header: MavHeader, msg: MavMessage) -> Result<()> {
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
                log::info!("航点信息:{item:?}");
                self.state = match self.state.clone() {
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
            // HEARTBEAT
            MavMessage::HEARTBEAT(heartbeat) => {
                log::info!("收到心跳: {:?}", heartbeat);
            }
            // 加解锁
            MavMessage::COMMAND_ACK(ack) => {
                log::info!("收到解锁/上锁响应: {:?}", ack);
                // match ack.result {
                //     MAV_RESULT::MAV_RESULT_ACCEPTED => {
                //         log::info!("解锁/上锁成功");
                //     }
                //     _ => {
                //         log::error!("解锁/上锁失败: {:?}", ack.result);
                //     }
                // }
            }
            _ => {}
        }

        Ok(())
    }

    async fn handle_request_waypoint_list(&mut self) -> Result<()> {
        log::info!("收到航点列表请求命令");
        let req = MavMessage::MISSION_REQUEST_LIST(MISSION_REQUEST_LIST_DATA {
            target_system: 1,
            target_component: 1,
        });
        if let Err(e) = self.vehicle.send_default(&req) {
            log::error!("发送 MISSION_REQUEST_LIST 失败: {}", e);
        } else {
            self.state = MissionState::WaitingCount;
        }
        Ok(())
    }

    async fn handle_request_waypoint(&mut self, seq: u16) -> Result<()> {
        log::info!("收到航点请求命令，序号: {}", seq);
        let req = MavMessage::MISSION_REQUEST_INT(MISSION_REQUEST_INT_DATA {
            target_system: 1,
            target_component: 1,
            seq,
        });
        if let Err(e) = self.vehicle.send_default(&req) {
            log::error!("发送 MISSION_REQUEST_INT 失败: {}", e);
        }
        Ok(())
    }
    async fn handle_arm_disarm(&mut self, arm: bool) -> Result<()> {
        log::info!("收到解锁/上锁命令: {}", arm);
        let msg = MavMessage::COMMAND_LONG(COMMAND_LONG_DATA {
            target_system: 1,                      // 动态获取或配置
            target_component: 1,                   // 飞控组件 ID 通常为 1
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
        log::info!("发送解锁/上锁命令: {:?}", msg);
        if let Err(e) = self.vehicle.send_default(&msg) {
            log::error!("发送解锁/上锁命令失败: {}", e);
        }
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
