由于 `MavMessage` 枚举包含超过 **250 种消息**，完整列出全部内容会非常长。为了方便查阅，我按照 **消息 ID 排序**，整理了每个消息的名称、ID 和简要说明。你可以使用浏览器的查找功能（Ctrl+F）快速定位。

如果需要某个特定消息的详细字段定义，请告诉我消息名称或 ID，我可以单独提供。

## 全部 MavMessage 枚举完整列表（按 ID 排序）

| ID   | 消息名称                                  | 简要说明                                                           |
|------|------------------------------------------|--------------------------------------------------------------------|
| 0    | HEARTBEAT                                | 心跳包，表明系统在线，包含系统类型、状态等基础信息                   |
| 1    | SYS_STATUS                               | 系统总体状态（电池、CPU负载、通信丢失等）                           |
| 2    | SYSTEM_TIME                              | 系统时间（Unix 时间戳或启动后纳秒数）                               |
| 4    | PING                                     | 测量链路延迟（已弃用，建议使用 TIMESYNC）                           |
| 5    | CHANGE_OPERATOR_CONTROL                  | 请求控制此 MAV                                                      |
| 6    | CHANGE_OPERATOR_CONTROL_ACK              | 接受/拒绝控制 MAV 的回复                                            |
| 7    | AUTH_KEY                                 | 加密签名/密钥（需加密通道）                                         |
| 8    | LINK_NODE_STATUS                         | 通信链中各节点生成的状态                                            |
| 11   | SET_MODE                                 | 设置系统模式（已弃用，使用 MAV_CMD_DO_SET_MODE）                    |
| 20   | PARAM_REQUEST_READ                       | 请求读取某个参数的值                                                |
| 21   | PARAM_REQUEST_LIST                       | 请求所有参数列表                                                    |
| 22   | PARAM_VALUE                              | 单个参数的名称和数值                                                |
| 23   | PARAM_SET                                | 设置某个参数的值                                                    |
| 24   | GPS_RAW_INT                              | GPS 原始数据（经纬度、海拔、卫星数、HDOP）                          |
| 25   | GPS_STATUS                               | GPS 卫星状态（每颗卫星的信噪比、使用情况）                          |
| 26   | SCALED_IMU                               | 校准后的 IMU 数据（加速度、角速度、磁力计）                         |
| 27   | RAW_IMU                                  | 原始 IMU 数据（未缩放）                                             |
| 28   | RAW_PRESSURE                             | 原始气压计数据（绝对气压、差压）                                    |
| 29   | SCALED_PRESSURE                          | 校准后的气压数据                                                    |
| 30   | ATTITUDE                                 | 姿态角（滚转、俯仰、偏航）                                          |
| 31   | ATTITUDE_QUATERNION                      | 四元数姿态                                                          |
| 32   | LOCAL_POSITION_NED                       | 局部位置（北、东、地坐标系）                                        |
| 33   | GLOBAL_POSITION_INT                      | 全局位置（经纬度、高度、地速、航向），整数格式                      |
| 34   | RC_CHANNELS_SCALED                       | 缩放后的 RC 通道值（-10000 到 10000）                               |
| 35   | RC_CHANNELS_RAW                          | RC 通道原始值（PPM）                                                |
| 36   | SERVO_OUTPUT_RAW                         | 舵机输出原始值                                                      |
| 37   | MISSION_REQUEST_PARTIAL_LIST             | 请求部分任务列表                                                    |
| 38   | MISSION_WRITE_PARTIAL_LIST               | 写入部分任务列表                                                    |
| 39   | MISSION_ITEM                             | 任务项（已弃用，使用 MISSION_ITEM_INT）                             |
| 40   | MISSION_REQUEST                          | 请求任务项（已弃用，使用 MISSION_REQUEST_INT）                      |
| 41   | MISSION_SET_CURRENT                      | 设置当前任务项（已弃用，使用 MAV_CMD_DO_SET_MISSION_CURRENT）       |
| 42   | MISSION_CURRENT                          | 当前正在执行的任务项序号                                            |
| 43   | MISSION_REQUEST_LIST                     | 请求完整任务列表                                                    |
| 44   | MISSION_COUNT                            | 告知对方任务项总数                                                  |
| 45   | MISSION_CLEAR_ALL                        | 清除所有任务项                                                      |
| 46   | MISSION_ITEM_REACHED                     | 已到达某个任务项                                                    |
| 47   | MISSION_ACK                              | 确认任务操作的结果                                                  |
| 48   | SET_GPS_GLOBAL_ORIGIN                    | 设置本地坐标系原点的 GPS 坐标（已弃用）                             |
| 49   | GPS_GLOBAL_ORIGIN                        | 本地坐标系原点的 GPS 坐标                                           |
| 50   | PARAM_MAP_RC                             | 将 RC 通道绑定到参数                                                |
| 51   | MISSION_REQUEST_INT                      | 请求任务项（整数坐标版本）                                          |
| 54   | SAFETY_SET_ALLOWED_AREA                  | 设置安全区域（立方体）                                              |
| 55   | SAFETY_ALLOWED_AREA                      | 读取当前安全区域                                                    |
| 61   | ATTITUDE_QUATERNION_COV                  | 带协方差的四元数姿态                                                |
| 62   | NAV_CONTROLLER_OUTPUT                    | 导航控制器输出（航向、距离、目标方位等）                            |
| 63   | GLOBAL_POSITION_INT_COV                  | 带协方差的全局位置                                                  |
| 64   | LOCAL_POSITION_NED_COV                   | 带协方差的局部位置                                                  |
| 65   | RC_CHANNELS                              | RC 通道信息（包含接收信号强度等）                                   |
| 66   | REQUEST_DATA_STREAM                      | 请求数据流（已弃用，使用 MAV_CMD_SET_MESSAGE_INTERVAL）             |
| 67   | DATA_STREAM                              | 数据流状态（已弃用）                                                |
| 69   | MANUAL_CONTROL                           | 手动控制量（摇杆）                                                  |
| 70   | RC_CHANNELS_OVERRIDE                     | 覆盖 RC 通道输出                                                    |
| 73   | MISSION_ITEM_INT                         | 任务项（整数经纬度，推荐）                                          |
| 74   | VFR_HUD                                  | 平视显示器数据（空速、地速、海拔、爬升率）                          |
| 75   | COMMAND_INT                              | 命令（整数坐标版本）                                                |
| 76   | COMMAND_LONG                             | 命令（浮点参数版本）                                                |
| 77   | COMMAND_ACK                              | 命令执行结果确认                                                    |
| 80   | COMMAND_CANCEL                           | 取消长时间运行的命令                                                |
| 81   | MANUAL_SETPOINT                          | 来自操作员的期望姿态（滚转、俯仰、偏航、推力）                      |
| 82   | SET_ATTITUDE_TARGET                      | 设置期望姿态（四元数或角速率）                                      |
| 83   | ATTITUDE_TARGET                          | 当前期望姿态（与 SET_ATTITUDE_TARGET 对应）                         |
| 84   | SET_POSITION_TARGET_LOCAL_NED            | 设置期望位置/速度/加速度（局部坐标系）                              |
| 85   | POSITION_TARGET_LOCAL_NED                | 当前期望位置/速度/加速度（局部坐标系）                              |
| 86   | SET_POSITION_TARGET_GLOBAL_INT           | 设置期望位置/速度/加速度（全局坐标系）                              |
| 87   | POSITION_TARGET_GLOBAL_INT               | 当前期望位置/速度/加速度（全局坐标系）                              |
| 89   | LOCAL_POSITION_NED_SYSTEM_GLOBAL_OFFSET  | 局部坐标系与全局坐标系的偏移                                        |
| 90   | HIL_STATE                                | 硬件在环状态（已弃用，使用 HIL_STATE_QUATERNION）                   |
| 91   | HIL_CONTROLS                             | 硬件在环控制输出                                                    |
| 92   | HIL_RC_INPUTS_RAW                        | 硬件在环 RC 输入原始值                                              |
| 93   | HIL_ACTUATOR_CONTROLS                    | 硬件在环舵机控制输出                                                |
| 100  | OPTICAL_FLOW                             | 光流数据                                                            |
| 101  | GLOBAL_VISION_POSITION_ESTIMATE          | 视觉全局位置估计                                                    |
| 102  | VISION_POSITION_ESTIMATE                 | 视觉局部位置估计                                                    |
| 103  | VISION_SPEED_ESTIMATE                    | 视觉速度估计                                                        |
| 104  | VICON_POSITION_ESTIMATE                  | Vicon 运动捕捉位置估计                                              |
| 105  | HIGHRES_IMU                              | 高分辨率 IMU 数据（SI 单位）                                        |
| 106  | OPTICAL_FLOW_RAD                         | 带角速度补偿的光流                                                  |
| 107  | HIL_SENSOR                               | 硬件在环传感器数据                                                  |
| 108  | SIM_STATE                                | 仿真状态（旧版）                                                    |
| 109  | RADIO_STATUS                             | 无线电状态（RSSI、噪声等）                                          |
| 110  | FILE_TRANSFER_PROTOCOL                   | 文件传输协议消息                                                    |
| 111  | TIMESYNC                                 | 时间同步消息                                                        |
| 112  | CAMERA_TRIGGER                           | 相机触发信号                                                        |
| 113  | HIL_GPS                                  | 硬件在环 GPS 数据                                                   |
| 114  | HIL_OPTICAL_FLOW                         | 硬件在环光流数据                                                    |
| 115  | HIL_STATE_QUATERNION                     | 硬件在环状态（四元数）                                              |
| 116  | SCALED_IMU2                              | 第二个 IMU 的校准数据                                               |
| 117  | LOG_REQUEST_LIST                         | 请求日志文件列表                                                    |
| 118  | LOG_ENTRY                                | 日志文件条目信息                                                    |
| 119  | LOG_REQUEST_DATA                         | 请求日志数据块                                                      |
| 120  | LOG_DATA                                 | 日志数据块                                                          |
| 121  | LOG_ERASE                                | 擦除所有日志                                                        |
| 122  | LOG_REQUEST_END                          | 结束日志传输                                                        |
| 123  | GPS_INJECT_DATA                          | 注入 GPS 数据（已弃用，使用 GPS_RTCM_DATA）                         |
| 124  | GPS2_RAW                                 | 第二个 GPS 的原始数据                                               |
| 125  | POWER_STATUS                             | 电源状态（电压、电流）                                              |
| 126  | SERIAL_CONTROL                           | 串口控制（波特率、数据流）                                          |
| 127  | GPS_RTK                                  | RTK GPS 基线信息                                                    |
| 128  | GPS2_RTK                                 | 第二个 RTK GPS 基线信息                                             |
| 129  | SCALED_IMU3                              | 第三个 IMU 的校准数据                                               |
| 130  | DATA_TRANSMISSION_HANDSHAKE              | 图像传输协议握手                                                    |
| 131  | ENCAPSULATED_DATA                        | 封装的数据（图像传输）                                              |
| 132  | DISTANCE_SENSOR                          | 测距仪数据                                                          |
| 133  | TERRAIN_REQUEST                          | 请求地形数据                                                        |
| 134  | TERRAIN_DATA                             | 地形数据块                                                          |
| 135  | TERRAIN_CHECK                            | 检查特定位置的地形数据                                              |
| 136  | TERRAIN_REPORT                           | 地形数据报告                                                        |
| 137  | SCALED_PRESSURE2                         | 第二个气压计的校准数据                                              |
| 138  | ATT_POS_MOCAP                            | 动作捕捉的位置和姿态                                                |
| 139  | SET_ACTUATOR_CONTROL_TARGET              | 设置执行器控制目标（角速度、推力等）                                |
| 140  | ACTUATOR_CONTROL_TARGET                  | 当前执行器控制目标                                                  |
| 141  | ALTITUDE                                 | 当前系统高度（相对、绝对、气压）                                    |
| 142  | RESOURCE_REQUEST                         | 请求资源（文件、二进制数据）                                        |
| 143  | SCALED_PRESSURE3                         | 第三个气压计的校准数据                                              |
| 144  | FOLLOW_TARGET                            | 跟随目标的位置信息                                                  |
| 146  | CONTROL_SYSTEM_STATE                     | 控制系统状态（用于反馈）                                            |
| 147  | BATTERY_STATUS                           | 电池状态（电压、电流、剩余容量）                                    |
| 148  | AUTOPILOT_VERSION                        | 飞控版本和功能                                                      |
| 149  | LANDING_TARGET                           | 着陆目标位置                                                        |
| 150  | SENSOR_OFFSETS                           | 传感器偏移校准（已弃用）                                            |
| 151  | SET_MAG_OFFSETS                          | 设置磁力计偏移（已弃用）                                            |
| 152  | MEMINFO                                  | 内存信息（剩余内存）                                                |
| 153  | AP_ADC                                   | 模数转换器原始值                                                    |
| 154  | DIGICAM_CONFIGURE                        | 配置相机                                                            |
| 155  | DIGICAM_CONTROL                          | 控制相机（拍照）                                                    |
| 156  | MOUNT_CONFIGURE                          | 配置云台                                                            |
| 157  | MOUNT_CONTROL                            | 控制云台角度                                                        |
| 158  | MOUNT_STATUS                             | 云台状态                                                            |
| 160  | FENCE_POINT                              | 围栏点（地理围栏）                                                  |
| 161  | FENCE_FETCH_POINT                        | 请求围栏点                                                          |
| 162  | FENCE_STATUS                             | 围栏状态                                                            |
| 163  | AHRS                                     | 姿态航向参考系统状态                                                |
| 164  | SIMSTATE                                 | 仿真状态（新版）                                                    |
| 165  | HWSTATUS                                 | 硬件状态（温度、电压等）                                            |
| 166  | RADIO                                    | 无线电状态（简版）                                                  |
| 167  | LIMITS_STATUS                            | 限制器状态（高度、距离等）                                          |
| 168  | WIND                                     | 风速估计                                                            |
| 169  | DATA16                                   | 16 字节数据包                                                       |
| 170  | DATA32                                   | 32 字节数据包                                                       |
| 171  | DATA64                                   | 64 字节数据包                                                       |
| 172  | DATA96                                   | 96 字节数据包                                                       |
| 173  | RANGEFINDER                              | 测距仪报告                                                          |
| 174  | AIRSPEED_AUTOCAL                         | 空速自动校准状态                                                    |
| 175  | RALLY_POINT                              | 集结点点                                                            |
| 176  | RALLY_FETCH_POINT                        | 请求集结点                                                          |
| 177  | COMPASSMOT_STATUS                        | 罗盘电机校准状态                                                    |
| 178  | AHRS2                                    | 第二个 AHRS 状态                                                    |
| 179  | CAMERA_STATUS                            | 相机事件状态                                                        |
| 180  | CAMERA_FEEDBACK                          | 相机拍照反馈                                                        |
| 181  | BATTERY2                                 | 第二个电池状态（已弃用）                                            |
| 182  | AHRS3                                    | 第三个 AHRS 状态                                                    |
| 183  | AUTOPILOT_VERSION_REQUEST                | 请求飞控版本                                                        |
| 184  | REMOTE_LOG_DATA_BLOCK                    | 远程日志数据块                                                      |
| 185  | REMOTE_LOG_BLOCK_STATUS                  | 远程日志块状态                                                      |
| 186  | LED_CONTROL                              | 控制 LED                                                            |
| 191  | MAG_CAL_PROGRESS                         | 罗盘校准进度                                                        |
| 192  | MAG_CAL_REPORT                           | 罗盘校准结果报告                                                    |
| 193  | EKF_STATUS_REPORT                        | 扩展卡尔曼滤波器状态                                                |
| 194  | PID_TUNING                               | PID 调谐信息                                                        |
| 195  | DEEPSTALL                                | 深度失速路径规划                                                    |
| 200  | GIMBAL_REPORT                            | 三轴云台测量值                                                      |
| 201  | GIMBAL_CONTROL                           | 云台控制（速率）                                                    |
| 214  | GIMBAL_TORQUE_CMD_REPORT                 | 云台扭矩命令报告                                                    |
| 215  | GOPRO_HEARTBEAT                          | GoPro 心跳                                                          |
| 216  | GOPRO_GET_REQUEST                        | 请求 GoPro 参数                                                     |
| 217  | GOPRO_GET_RESPONSE                       | GoPro 参数响应                                                      |
| 218  | GOPRO_SET_REQUEST                        | 设置 GoPro 参数                                                     |
| 219  | GOPRO_SET_RESPONSE                       | GoPro 参数设置响应                                                  |
| 225  | EFI_STATUS                               | 电子燃油喷射状态                                                    |
| 226  | RPM                                      | 转速传感器输出                                                      |
| 230  | ESTIMATOR_STATUS                         | 估计器状态（创新测试比、精度等）                                    |
| 231  | WIND_COV                                 | 风速估计（带标准差）                                                |
| 232  | GPS_INPUT                                | GPS 输入（供估计器使用）                                            |
| 233  | GPS_RTCM_DATA                            | RTCM 差分数据注入                                                   |
| 234  | HIGH_LATENCY                             | 高延迟链路消息（已弃用，使用 HIGH_LATENCY2）                        |
| 235  | HIGH_LATENCY2                            | 高延迟链路消息（版本2）                                             |
| 241  | VIBRATION                                | 振动和加速度计限幅情况                                              |
| 242  | HOME_POSITION                            | 家位置（经纬度、高度、朝向）                                        |
| 243  | SET_HOME_POSITION                        | 设置家位置（已弃用）                                                |
| 244  | MESSAGE_INTERVAL                         | 消息发送间隔                                                        |
| 245  | EXTENDED_SYS_STATE                       | 扩展系统状态（VTOL、着陆等）                                        |
| 246  | ADSB_VEHICLE                             | ADSB 飞行器信息                                                     |
| 247  | COLLISION                                | 潜在碰撞信息                                                        |
| 248  | V2_EXTENSION                             | V2 扩展（在 V1 帧中传输 V2 载荷）                                   |
| 249  | MEMORY_VECT                              | 内存向量（调试）                                                    |
| 250  | DEBUG_VECT                               | 调试向量（3D）                                                      |
| 251  | NAMED_VALUE_FLOAT                        | 命名浮点调试值                                                      |
| 252  | NAMED_VALUE_INT                          | 命名整数调试值                                                      |
| 253  | STATUSTEXT                               | 状态文本消息                                                        |
| 254  | DEBUG                                    | 调试值（索引区分）                                                  |
| 256  | SETUP_SIGNING                            | 设置 MAVLink 签名密钥                                               |
| 257  | BUTTON_CHANGE                            | 按钮状态变化                                                        |
| 258  | PLAY_TUNE                                | 播放音调（已弃用，使用 PLAY_TUNE_V2）                               |
| 259  | CAMERA_INFORMATION                       | 相机信息（型号、能力）                                              |
| 260  | CAMERA_SETTINGS                          | 相机设置（模式、缩放等）                                            |
| 261  | STORAGE_INFORMATION                      | 存储介质信息（SD 卡容量、剩余空间）                                 |
| 262  | CAMERA_CAPTURE_STATUS                    | 相机拍摄状态                                                        |
| 263  | CAMERA_IMAGE_CAPTURED                    | 已拍摄图像信息                                                      |
| 264  | FLIGHT_INFORMATION                       | 飞行信息（起飞时间、飞行编号）                                      |
| 265  | MOUNT_ORIENTATION                        | 云台朝向（已弃用）                                                  |
| 266  | LOGGING_DATA                             | 日志数据                                                            |
| 267  | LOGGING_DATA_ACKED                       | 需要确认的日志数据                                                  |
| 268  | LOGGING_ACK                              | 日志数据确认                                                        |
| 269  | VIDEO_STREAM_INFORMATION                 | 视频流信息                                                          |
| 270  | VIDEO_STREAM_STATUS                      | 视频流状态                                                          |
| 271  | CAMERA_FOV_STATUS                        | 相机视场角状态                                                      |
| 275  | CAMERA_TRACKING_IMAGE_STATUS             | 相机图像跟踪状态                                                    |
| 276  | CAMERA_TRACKING_GEO_STATUS               | 相机地理跟踪状态                                                    |
| 277  | CAMERA_THERMAL_RANGE                     | 相机热成像范围                                                      |
| 280  | GIMBAL_MANAGER_INFORMATION               | 云台管理器信息                                                      |
| 281  | GIMBAL_MANAGER_STATUS                    | 云台管理器状态                                                      |
| 282  | GIMBAL_MANAGER_SET_ATTITUDE              | 设置云台管理器期望姿态                                              |
| 283  | GIMBAL_DEVICE_INFORMATION                | 云台设备信息                                                        |
| 284  | GIMBAL_DEVICE_SET_ATTITUDE               | 设置云台设备期望姿态                                                |
| 285  | GIMBAL_DEVICE_ATTITUDE_STATUS            | 云台设备姿态状态                                                    |
| 286  | AUTOPILOT_STATE_FOR_GIMBAL_DEVICE        | 飞控提供给云台的状态（地平线补偿等）                                |
| 287  | GIMBAL_MANAGER_SET_PITCHYAW              | 设置云台管理器俯仰/偏航（高频率）                                   |
| 288  | GIMBAL_MANAGER_SET_MANUAL_CONTROL        | 手动控制云台                                                        |
| 290  | ESC_INFO                                 | 电子调速器信息（低频率）                                            |
| 291  | ESC_STATUS                               | 电子调速器状态（高频率）                                            |
| 299  | WIFI_CONFIG_AP                           | 配置 WiFi 接入点                                                    |
| 300  | PROTOCOL_VERSION                         | 协议版本协商                                                        |
| 301  | AIS_VESSEL                               | AIS 船舶信息                                                        |
| 310  | UAVCAN_NODE_STATUS                       | UAVCAN 节点状态                                                     |
| 311  | UAVCAN_NODE_INFO                         | UAVCAN 节点信息                                                     |
| 320  | PARAM_EXT_REQUEST_READ                   | 请求扩展参数值                                                      |
| 321  | PARAM_EXT_REQUEST_LIST                   | 请求所有扩展参数列表                                                |
| 322  | PARAM_EXT_VALUE                          | 扩展参数值                                                          |
| 323  | PARAM_EXT_SET                            | 设置扩展参数                                                        |
| 324  | PARAM_EXT_ACK                            | 扩展参数设置确认                                                    |
| 330  | OBSTACLE_DISTANCE                        | 障碍物距离（扇形）                                                  |
| 331  | ODOMETRY                                 | 里程计信息（符合 ROS REP 147）                                      |
| 332  | TRAJECTORY_REPRESENTATION_WAYPOINTS      | 航点轨迹表示                                                        |
| 333  | TRAJECTORY_REPRESENTATION_BEZIER         | 贝塞尔曲线轨迹表示                                                  |
| 334  | CELLULAR_STATUS                          | 蜂窝网络状态                                                        |
| 335  | ISBD_LINK_STATUS                         | 铱星 SBD 链路状态                                                   |
| 336  | CELLULAR_CONFIG                          | 蜂窝网络配置                                                        |
| 339  | RAW_RPM                                  | 原始转速传感器数据                                                  |
| 340  | UTM_GLOBAL_POSITION                      | UTM 坐标系下的全局位置                                              |
| 350  | DEBUG_FLOAT_ARRAY                        | 调试浮点数组                                                        |
| 360  | ORBIT_EXECUTION_STATUS                   | 轨道执行状态                                                        |
| 370  | SMART_BATTERY_INFO                       | 智能电池信息（已弃用）                                              |
| 371  | FUEL_STATUS                              | 燃料状态                                                            |
| 372  | BATTERY_INFO                             | 电池静态信息                                                        |
| 373  | GENERATOR_STATUS                         | 发电机状态                                                          |
| 375  | ACTUATOR_OUTPUT_STATUS                   | 执行器输出状态（舵机、电机等）                                      |
| 380  | TIME_ESTIMATE_TO_TARGET                  | 到达目标的时间估计                                                  |
| 385  | TUNNEL                                   | 隧道数据（任意数据）                                                |
| 386  | CAN_FRAME                                | CAN 帧转发                                                          |
| 387  | CANFD_FRAME                              | CAN FD 帧转发                                                       |
| 388  | CAN_FILTER_MODIFY                        | 修改 CAN 转发过滤                                                   |
| 390  | ONBOARD_COMPUTER_STATUS                  | 机载计算机状态                                                      |
| 395  | COMPONENT_INFORMATION                    | 组件信息（已弃用）                                                  |
| 396  | COMPONENT_INFORMATION_BASIC              | 组件基本信息                                                        |
| 397  | COMPONENT_METADATA                       | 组件元数据                                                          |
| 400  | PLAY_TUNE_V2                             | 播放音调（版本2）                                                   |
| 401  | SUPPORTED_TUNES                          | 支持的音调格式                                                      |
| 410  | EVENT                                    | 事件消息                                                            |
| 411  | CURRENT_EVENT_SEQUENCE                   | 当前事件序号                                                        |
| 412  | REQUEST_EVENT                            | 请求重发事件                                                        |
| 413  | RESPONSE_EVENT_ERROR                     | 事件错误响应                                                        |
| 435  | AVAILABLE_MODES                          | 可用飞行模式信息                                                    |
| 436  | CURRENT_MODE                             | 当前飞行模式                                                        |
| 437  | AVAILABLE_MODES_MONITOR                  | 可用模式变更监视                                                    |
| 440  | ILLUMINATOR_STATUS                       | 照明器状态                                                          |
| 9000 | WHEEL_DISTANCE                           | 轮式里程计距离                                                      |
| 9005 | WINCH_STATUS                             | 绞盘状态                                                            |
| 10001| UAVIONIX_ADSB_OUT_CFG                    | UAVionix ADS-B 静态配置                                             |
| 10002| UAVIONIX_ADSB_OUT_DYNAMIC                | UAVionix ADS-B 动态数据                                             |
| 10003| UAVIONIX_ADSB_TRANSCEIVER_HEALTH_REPORT  | UAVionix ADS-B 收发器健康报告                                       |
| 10004| UAVIONIX_ADSB_OUT_CFG_REGISTRATION       | UAVionix ADS-B 注册信息                                             |
| 10005| UAVIONIX_ADSB_OUT_CFG_FLIGHTID           | UAVionix ADS-B 航班号                                               |
| 10006| UAVIONIX_ADSB_GET                        | 请求 UAVionix ADS-B 配置                                            |
| 10007| UAVIONIX_ADSB_OUT_CONTROL                | UAVionix ADS-B 控制                                                 |
| 10008| UAVIONIX_ADSB_OUT_STATUS                 | UAVionix ADS-B 状态                                                 |
| 10151| LOWEHEISER_GOV_EFI                       | Loweheiser 发动机 EFI 与调速器数据                                  |
| 11000| DEVICE_OP_READ                           | 读取设备寄存器                                                      |
| 11001| DEVICE_OP_READ_REPLY                     | 设备寄存器读取回复                                                  |
| 11002| DEVICE_OP_WRITE                          | 写入设备寄存器                                                      |
| 11003| DEVICE_OP_WRITE_REPLY                    | 设备寄存器写入回复                                                  |
| 11004| SECURE_COMMAND                           | 安全命令（加密）                                                    |
| 11005| SECURE_COMMAND_REPLY                     | 安全命令回复                                                        |
| 11010| ADAP_TUNING                              | 自适应控制器调谐信息                                                |
| 11011| VISION_POSITION_DELTA                    | 视觉位置增量                                                        |
| 11020| AOA_SSA                                  | 攻角和侧滑角                                                        |
| 11030| ESC_TELEMETRY_1_TO_4                     | ESC 遥测 1-4 号                                                     |
| 11031| ESC_TELEMETRY_5_TO_8                     | ESC 遥测 5-8 号                                                     |
| 11032| ESC_TELEMETRY_9_TO_12                    | ESC 遥测 9-12 号                                                    |
| 11033| OSD_PARAM_CONFIG                         | OSD 参数配置                                                        |
| 11034| OSD_PARAM_CONFIG_REPLY                   | OSD 参数配置回复                                                    |
| 11035| OSD_PARAM_SHOW_CONFIG                    | 读取 OSD 参数配置                                                   |
| 11036| OSD_PARAM_SHOW_CONFIG_REPLY              | OSD 参数配置读取回复                                                |
| 11037| OBSTACLE_DISTANCE_3D                     | 3D 障碍物距离                                                       |
| 11038| WATER_DEPTH                              | 水深                                                                |
| 11039| MCU_STATUS                               | 微控制器状态（温度、电压）                                          |
| 11040| ESC_TELEMETRY_13_TO_16                   | ESC 遥测 13-16 号                                                   |
| 11041| ESC_TELEMETRY_17_TO_20                   | ESC 遥测 17-20 号                                                   |
| 11042| ESC_TELEMETRY_21_TO_24                   | ESC 遥测 21-24 号                                                   |
| 11043| ESC_TELEMETRY_25_TO_28                   | ESC 遥测 25-28 号                                                   |
| 11044| ESC_TELEMETRY_29_TO_32                   | ESC 遥测 29-32 号                                                   |
| 12900| OPEN_DRONE_ID_BASIC_ID                   | 远程 ID 基础信息                                                    |
| 12901| OPEN_DRONE_ID_LOCATION                   | 远程 ID 位置信息                                                    |
| 12902| OPEN_DRONE_ID_AUTHENTICATION             | 远程 ID 认证信息                                                    |
| 12903| OPEN_DRONE_ID_SELF_ID                    | 远程 ID 自声明信息                                                  |
| 12904| OPEN_DRONE_ID_SYSTEM                     | 远程 ID 系统信息                                                    |
| 12905| OPEN_DRONE_ID_OPERATOR_ID                | 远程 ID 操作员 ID                                                   |
| 12915| OPEN_DRONE_ID_MESSAGE_PACK               | 远程 ID 消息包（压缩格式）                                          |
| 12918| OPEN_DRONE_ID_ARM_STATUS                 | 远程 ID 解锁状态                                                    |
| 12919| OPEN_DRONE_ID_SYSTEM_UPDATE              | 远程 ID 系统信息更新                                                |
| 12920| HYGROMETER_SENSOR                        | 温湿度传感器                                                        |
| 42000| ICAROUS_HEARTBEAT                        | ICAROUS 心跳                                                        |
| 42001| ICAROUS_KINEMATIC_BANDS                  | ICAROUS 运动波段                                                    |
| 50001| CUBEPILOT_RAW_RC                         | CubePilot 原始 RC 数据                                              |
| 50002| HERELINK_VIDEO_STREAM_INFORMATION        | HereLink 视频流信息                                                 |
| 50003| HERELINK_TELEM                           | HereLink 遥测                                                       |
| 50004| CUBEPILOT_FIRMWARE_UPDATE_START          | CubePilot 固件更新开始                                              |
| 50005| CUBEPILOT_FIRMWARE_UPDATE_RESP           | CubePilot 固件更新响应                                              |

---

如果需要某个消息的详细字段定义（例如 `MISSION_ITEM_INT` 的每个字段含义），请告诉我消息名称或 ID，我会单独提供完整表格。