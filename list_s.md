这些 MAVLink 消息几乎涵盖了无人机（或机器人）从起飞到降落的全部通信需求。下面按**实际应用场景**分类说明，帮助你理解每类消息在真实系统中发挥的作用。

---

## 1. 系统基础与状态监控
**场景**：地面站连接飞控后，持续了解系统是否在线、健康状态、模式等。  
**典型消息**：  
- `HEARTBEAT` – 定期广播，地面站据此判断设备是否“活着”，并显示系统类型（四旋翼、固定翼等）和飞行模式。  
- `SYS_STATUS` – 提供电池电压/电流、CPU负载、通信丢失计数等，用于地面站仪表盘显示。  
- `SYSTEM_TIME` – 同步机载时间，便于日志分析和多设备时间对齐。  
- `STATUSTEXT` – 飞控发送的文本消息（如“PreArm: Compass not calibrated”），地面站显示在控制台。

---

## 2. 任务规划与执行（航点/任务）
**场景**：上传飞行任务（航点、拍照、着陆等），下载现有任务，监控执行进度。  
**典型消息**：  
- `MISSION_COUNT` – 上传前告知飞控总共有多少个任务项。  
- `MISSION_ITEM_INT` – 单个任务项（经纬度、高度、命令类型如“航点”、“悬停”、“降落”）。  
- `MISSION_REQUEST_INT` – 飞控请求下一个任务项（遵循请求-响应模式）。  
- `MISSION_ACK` – 确认任务上传/清除/设置成功或失败。  
- `MISSION_CURRENT` – 飞控广播当前正在执行的任务序号，地面站可高亮显示。

**应用举例**：  
地面站规划一条包含10个航点的航线，点击“上传”。地面站先发送 `MISSION_COUNT=10`，飞控回复 `MISSION_REQUEST_INT seq=0`，地面站发送第0个航点，飞控再请求第1个，直到全部发送完毕，飞控回复 `MISSION_ACK`。飞行中飞控持续广播 `MISSION_CURRENT`，地面站可在地图上动态指示当前位置。

---

## 3. 参数配置与校准
**场景**：读写飞控参数（PID、传感器校准系数、EKF设置等），进行传感器校准。  
**典型消息**：  
- `PARAM_REQUEST_LIST` – 地面站请求所有参数，飞控回复多个 `PARAM_VALUE`。  
- `PARAM_VALUE` – 单个参数名称和值（例如“RC1_MIN”=1100）。  
- `PARAM_SET` – 修改参数值并保存到飞控。  
- `PARAM_EXT_*` – 用于较长参数名或浮点数组等扩展参数。  
- `MAG_CAL_PROGRESS` / `MAG_CAL_REPORT` – 罗盘校准进度和结果。

**应用举例**：  
校准加速度计时，地面站发送 `MAV_CMD_PREFLIGHT_CALIBRATION` 命令（通过 `COMMAND_LONG`），飞控执行并发送 `STATUSTEXT` 提示步骤。地面站也可直接读取/修改 `CAL_ACC*` 参数。

---

## 4. 传感器数据获取（遥测）
**场景**：实时获取位置、姿态、速度、IMU、GPS、空速、测距仪等数据，用于显示和控制。  
**典型消息**：  
- `GLOBAL_POSITION_INT` – 经纬度、高度、地速、航向，地面站地图显示。  
- `ATTITUDE` – 滚转/俯仰/偏航角，姿态仪表盘。  
- `VFR_HUD` – 空速、地速、海拔、爬升率，平视显示器数据。  
- `GPS_RAW_INT` – 原始GPS数据（卫星数、HDOP等），评估定位质量。  
- `SCALED_IMU` – 加速度、角速度、磁力计，可用于数据记录或高级分析。  
- `DISTANCE_SENSOR` – 超声波/激光雷达距离，用于低空悬停或避障。  
- `BATTERY_STATUS` – 单体电压、剩余容量、电流，电池报警。

**应用举例**：  
地面站每隔100ms收到一次 `GLOBAL_POSITION_INT`，在地图上更新飞机图标；收到 `BATTERY_STATUS` 后显示剩余电量百分比，低于30%时弹出低电量警告。

---

## 5. 遥控与手动控制
**场景**：手动控制飞机（摇杆输入）、设置模式、执行紧急动作（例如降落）。  
**典型消息**：  
- `RC_CHANNELS_RAW` – 接收机输出的PPM值，地面站可用于显示摇杆位置。  
- `RC_CHANNELS_OVERRIDE` – 地面站通过它直接控制舵机/电机（常用于自动起飞或紧急干预）。  
- `MANUAL_CONTROL` – 标准化摇杆输入（roll/pitch/yaw/throttle），用于操纵杆设备。  
- `COMMAND_LONG` – 发送 `MAV_CMD_COMPONENT_ARM_DISARM`（解锁/上锁）、`MAV_CMD_NAV_TAKEOFF`、`MAV_CMD_NAV_RETURN_TO_LAUNCH`（返航）。

**应用举例**：  
地面站点击“解锁”按钮 → 发送 `COMMAND_LONG` 包含 `MAV_CMD_COMPONENT_ARM_DISARM` → 飞控回复 `COMMAND_ACK`。解锁成功后推杆（发送 `MANUAL_CONTROL`）飞机即可起飞。

---

## 6. 位置估计与导航
**场景**：融合GPS、视觉、激光雷达等数据，提高定位精度，尤其是无GPS环境。  
**典型消息**：  
- `VISION_POSITION_ESTIMATE` – 视觉里程计提供的位置/姿态，用于GPS拒止环境。  
- `ODOMETRY` – 符合ROS规范的里程计（位置、速度、协方差）。  
- `LOCAL_POSITION_NED` – 局部坐标系下的位置（例如从视觉SLAM获得）。  
- `SET_POSITION_TARGET_LOCAL_NED` – 外部控制器（如机载计算机）发送期望位置，飞控跟踪。

**应用举例**：  
室内无人机使用VIO（视觉惯性里程计）定位，机载电脑运行VIO算法，通过 `VISION_POSITION_ESTIMATE` 发送位置/姿态给飞控，飞控融合内部IMU后实现稳定悬停。

---

## 7. 相机与云台控制
**场景**：拍照、录像、调整相机参数、控制云台指向。  
**典型消息**：  
- `CAMERA_IMAGE_CAPTURED` – 相机拍照后通知地面站，附图像保存路径和GPS坐标。  
- `CAMERA_INFORMATION` – 相机型号、分辨率、是否支持云台。  
- `GIMBAL_MANAGER_SET_ATTITUDE` – 设置云台朝向（相对航向或绝对北）。  
- `VIDEO_STREAM_INFORMATION` – 视频流URL（RTSP等），地面站可自动打开播放器。

**应用举例**：  
电力巡检任务中，当飞机飞到指定塔杆时，地面站发送 `MAV_CMD_IMAGE_START_CAPTURE` 触发拍照，相机回复 `CAMERA_IMAGE_CAPTURED`，地面站将照片与GPS坐标绑定存入数据库。

---

## 8. 安全与地理围栏
**场景**：限制飞行区域，防止飞越禁飞区；远程识别（Remote ID）合规。  
**典型消息**：  
- `FENCE_POINT` – 定义多边形围栏顶点。  
- `FENCE_STATUS` – 围栏触发状态（是否越界）。  
- `OPEN_DRONE_ID_*` 系列 – 根据ASTM F3411标准广播无人机ID、位置、操作员信息，满足远程ID法规。

**应用举例**：  
在机场附近飞行前，地面站上传一个围绕跑道的圆形围栏，飞控收到后如果GPS位置超出围栏，自动执行返航或降落。同时 `OPEN_DRONE_ID_LOCATION` 每秒广播位置，附近接收器可接收以识别无人机。

---

## 9. 日志与调试
**场景**：下载飞行日志、实时调试数值。  
**典型消息**：  
- `LOG_REQUEST_LIST` / `LOG_ENTRY` / `LOG_REQUEST_DATA` / `LOG_DATA` – 列出日志文件并分块下载。  
- `DEBUG` / `NAMED_VALUE_FLOAT` – 发送开发者自定义的调试值，用于实时曲线显示。  
- `MEMINFO` – 飞控剩余内存，诊断资源使用。

**应用举例**：  
飞行后分析，地面站通过MAVLink FTP或日志传输协议下载飞控内部存储的 `.BIN` 日志文件，用于事故分析。

---

## 10. 高延迟通信（卫星、蜂窝）
**场景**：通过Iridium、4G等低带宽、高延迟链路保持连接。  
**典型消息**：  
- `HIGH_LATENCY2` – 压缩的关键数据（位置、姿态、电池、任务状态），数据量小但可容忍秒级延迟。  
- `CELLULAR_STATUS` – 4G信号强度、运营商等。  
- `ISBD_LINK_STATUS` – 铱星SBD链路状态。

**应用举例**：  
超视距无人机通过铱星模块回传位置和剩余电量，地面站使用 `HIGH_LATENCY2` 每15秒更新一次地图位置。

---

## 11. 特定硬件集成
**场景**：集成ADSB-IN/OUT、UAVCAN设备、高级电源管理、ESC遥测等。  
**典型消息**：  
- `ADSB_VEHICLE` – 接收附近民航飞机的ADS-B广播，用于避让。  
- `UAVCAN_NODE_STATUS` – 监控UAVCAN总线上的设备（如电调、测距仪）健康状态。  
- `ESC_TELEMETRY_1_TO_4` – 获取电调的转速、电流、温度。  
- `GENERATOR_STATUS` – 油电混合发动机的发电机状态。  

**应用举例**：  
大型货运无人机安装UAVCAN电调，飞控通过 `ESC_TELEMETRY_1_TO_4` 读取四个电机转速，地面站显示单个电机异常时可紧急降落。

---

## 12. 仿真与硬件在环（HIL）
**场景**：在电脑上模拟飞控和传感器，测试控制算法或任务逻辑。  
**典型消息**：  
- `HIL_SENSOR` / `HIL_GPS` – 仿真器向飞控注入模拟传感器数据。  
- `HIL_ACTUATOR_CONTROLS` – 飞控计算出的舵机/电机值回传给仿真器，用于驱动虚拟模型。  
- `SIM_STATE` – 仿真器状态（位置、姿态、速度）。

**应用举例**：  
开发新型控制器时，先在Gazebo中模拟环境，运行真实飞控代码（硬件在环或软件在环），通过HIL消息交换数据，验证控制逻辑后再上真机。

---

## 总结：如何根据场景选择消息？

| 你的需求                     | 应该关注的消息类别                     |
|----------------------------|---------------------------------------|
| 连接与状态显示               | HEARTBEAT, SYS_STATUS, STATUSTEXT      |
| 规划自动航线                 | MISSION_*                              |
| 调整飞控参数                 | PARAM_*, CAL_* (通过 COMMAND_LONG)      |
| 实时显示飞机位置/姿态         | GLOBAL_POSITION_INT, ATTITUDE, VFR_HUD |
| 手动遥控                     | MANUAL_CONTROL, RC_CHANNELS_OVERRIDE   |
| 室内/无GPS定位               | VISION_POSITION_ESTIMATE, ODOMETRY     |
| 控制相机/云台                | CAMERA_*, GIMBAL_*                     |
| 确保安全合规                 | FENCE_*, OPEN_DRONE_ID_*               |
| 分析飞行日志                 | LOG_*, PARAM_VALUE (事后)              |
| 集成特殊外设                 | ESC_TELEMETRY_*, ADSB_*, UAVCAN_*      |

如果你正在开发某个具体功能（例如“自动起飞并拍照”），我可以给出对应的消息序列示例。