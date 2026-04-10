<script setup lang="ts">
import mqtt from "mqtt"
import { ref, reactive, onMounted, computed, onBeforeUnmount } from "vue"

// ========== MQTT 配置 ==========
const BROKER = "ws://101.200.223.8:8083/mqtt"
const TOPIC_MAVLINK = "mavlink/incoming"
const TOPIC_GET_LIST = "get_list"
const TOPIC_SEND = "send"
const IDLE_TIMEOUT_MS = 5000

// ========== 响应式状态 ==========
const connectionStatus = ref<"connecting" | "online" | "offline" | "error">("connecting")
const lastUpdateTime = ref("--:--:--")

const telemetry = reactive({
  roll_deg: null as number | null,
  pitch_deg: null as number | null,
  yaw_deg: null as number | null,
  groundspeed: null as number | null,
  airspeed: null as number | null,
  heading: null as number | null,
  climb: null as number | null,
  lat: null as number | null,
  lon: null as number | null,
  relative_alt: null as number | null,
  alt_msl: null as number | null,
  voltage: null as number | null,
  current: null as number | null,
  battery_remaining: null as number | null,
  consumed: null as number | null,
  fix_type: null as number | null,
  satellites: null as number | null,
  eph: null as number | null,
  armed: false,
  mode: "---",
  ekf_flags: null as number | null,
  mission_seq: 0,
  mission_total: 0,
  current_cmd: "",
  vib: { x: 0, y: 0, z: 0 },
  temperature: null as number | null,
  press_abs: null as number | null,
  load: null as number | null
})

// 航点相关状态
const isLoadingWaypoints = ref(false)
const waypointsMap = ref<Map<number, Waypoint>>(new Map())
const expectedTotal = ref(0)
const waypointsStartTime = ref(0)
const waypointStats = reactive({
  elapsed: 0,
  received: 0,
  rate: 0,
  status: ""
})

interface Waypoint {
  seq: number
  lat: number
  lon: number
  alt: number
}

// 飞行模式选项
const flightModeOptions = [
  { label: "手动模式 (MANUAL)", value: "MANUAL" },
  { label: "定高模式 (ALT_HOLD)", value: "ALT_HOLD" },
  { label: "悬停模式 (HOLD)", value: "HOLD" },
  { label: "自动模式 (AUTO)", value: "AUTO" },
  { label: "返航模式 (RTL)", value: "RTL" },
  { label: "降落模式 (LAND)", value: "LAND" },
  { label: "特技模式 (ACRO)", value: "ACRO" },
  { label: "位置保持 (POSHOLD)", value: "POSHOLD" }
]

const selectedMode = ref("MANUAL")

// 计算属性
const fixTypeText = computed(() => {
  const fixMap: Record<number, string> = {
    0: "无GPS",
    1: "无定位",
    2: "2D定位",
    3: "3D定位",
    4: "GPS+惯导",
    5: "RTK浮动",
    6: "RTK固定"
  }
  return telemetry.fix_type !== null ? fixMap[telemetry.fix_type] || `类型${telemetry.fix_type}` : "无定位"
})

const ekfStatusText = computed(() => {
  return telemetry.ekf_flags !== null ? (telemetry.ekf_flags & 0x01 ? "正常" : "异常") : "未知"
})

const waypointsData = computed(() => {
  return Array.from(waypointsMap.value.values())
    .sort((a, b) => a.seq - b.seq)
    .map((wp) => ({
      key: wp.seq,
      seq: wp.seq,
      lat: wp.lat.toFixed(6),
      lon: wp.lon.toFixed(6),
      alt: wp.alt.toFixed(2)
    }))
})

const waypointsColumns = [
  { title: "序号", key: "seq" },
  { title: "纬度", key: "lat" },
  { title: "经度", key: "lon" },
  { title: "高度(m)", key: "alt" }
]

// 辅助函数
function extractNumber(value: any, defaultValue: number | null = null): number | null {
  if (value === undefined || value === null) return defaultValue
  if (typeof value === "number") return value
  if (typeof value === "object") {
    if ("value" in value) return Number(value.value)
    if ("_value" in value) return Number(value._value)
    const keys = Object.keys(value)
    if (keys.length > 0 && typeof value[keys[0]] === "number") return value[keys[0]]
    const num = Number(value)
    if (!isNaN(num)) return num
  }
  if (typeof value === "string") {
    const num = Number(value)
    if (!isNaN(num)) return num
    if (value.includes("NO_GPS")) return 0
    if (value.includes("FIX_2D")) return 2
    if (value.includes("FIX_3D")) return 3
    return defaultValue
  }
  return defaultValue
}

function extractString(value: any, defaultValue: string = "---"): string {
  if (value === undefined || value === null) return defaultValue
  if (typeof value === "string") return value
  if (typeof value === "number") return value.toString()
  if (typeof value === "object") {
    if ("name" in value) return value.name
    const keys = Object.keys(value)
    if (keys.length > 0 && typeof value[keys[0]] === "string") return value[keys[0]]
    return JSON.stringify(value)
  }
  return defaultValue
}

function radToDeg(rad: number | null): string | null {
  if (rad === null || isNaN(rad)) return null
  return ((rad * 180) / Math.PI).toFixed(1)
}

function updateLastUpdateTime() {
  const now = new Date()
  lastUpdateTime.value = now.toLocaleTimeString("zh-CN", { hour12: false })
}

// 遥测消息处理（重点修改 HEARTBEAT 分支以适配后端新结构）
function processMavlinkMessage(payloadStr: string) {
  try {
    const obj = JSON.parse(payloadStr)
    const msgType = obj.message_type
    const data = obj.data || {}
    console.log(obj, "===================")
    switch (msgType) {
      case "ATTITUDE":
        if (data.roll !== undefined) telemetry.roll_deg = radToDeg(extractNumber(data.roll))
        if (data.pitch !== undefined) telemetry.pitch_deg = radToDeg(extractNumber(data.pitch))
        if (data.yaw !== undefined) telemetry.yaw_deg = radToDeg(extractNumber(data.yaw))
        break
      case "AHRS2":
        if (data.roll !== undefined) telemetry.roll_deg = radToDeg(extractNumber(data.roll))
        if (data.pitch !== undefined) telemetry.pitch_deg = radToDeg(extractNumber(data.pitch))
        if (data.yaw !== undefined) telemetry.yaw_deg = radToDeg(extractNumber(data.yaw))
        if (data.altitude !== undefined) telemetry.relative_alt = extractNumber(data.altitude) * 1000
        break
      case "VFR_HUD":
        if (data.airspeed !== undefined) telemetry.airspeed = extractNumber(data.airspeed)
        if (data.groundspeed !== undefined) telemetry.groundspeed = extractNumber(data.groundspeed)
        if (data.heading !== undefined) telemetry.heading = extractNumber(data.heading)
        if (data.climb !== undefined) telemetry.climb = extractNumber(data.climb)
        if (data.alt !== undefined) telemetry.relative_alt = extractNumber(data.alt) * 1000
        break
      case "GLOBAL_POSITION_INT":
        if (data.lat !== undefined) {
          let latVal = extractNumber(data.lat)
          if (latVal !== null) telemetry.lat = latVal
        }
        if (data.lon !== undefined) {
          let lonVal = extractNumber(data.lon)
          if (lonVal !== null) telemetry.lon = lonVal
        }
        if (data.alt !== undefined) telemetry.alt_msl = extractNumber(data.alt)
        if (data.relative_alt !== undefined) telemetry.relative_alt = extractNumber(data.relative_alt)
        if (data.hdg !== undefined) telemetry.heading = extractNumber(data.hdg) / 100
        break
      case "SYS_STATUS":
        if (data.voltage_battery !== undefined) telemetry.voltage = extractNumber(data.voltage_battery)
        if (data.current_battery !== undefined) telemetry.current = extractNumber(data.current_battery)
        if (data.battery_remaining !== undefined) telemetry.battery_remaining = extractNumber(data.battery_remaining)
        console.log("负载", data)
        if (data.load !== undefined) telemetry.load = extractNumber(data.load)
        break
      case "BATTERY_STATUS":
        if (data.voltages && Array.isArray(data.voltages) && data.voltages[0] !== 65535)
          telemetry.voltage = extractNumber(data.voltages[0])
        if (data.current_battery !== undefined) telemetry.current = extractNumber(data.current_battery)
        if (data.battery_remaining !== undefined) telemetry.battery_remaining = extractNumber(data.battery_remaining)
        if (data.current_consumed !== undefined) telemetry.consumed = extractNumber(data.current_consumed)
        break
      case "GPS_RAW_INT":
        let latVal = extractNumber(data.lat)
        let lonVal = extractNumber(data.lon)
        if (latVal !== null) telemetry.lat = latVal
        if (lonVal !== null) telemetry.lon = lonVal
        telemetry.fix_type = extractNumber(data.fix_type)
        telemetry.satellites = extractNumber(data.satellites_visible)
        telemetry.eph = extractNumber(data.eph)
        break
      // ========== 适配后端新结构 ==========
      case "HEARTBEAT":
        // 1. 解锁状态：直接使用 is_armed 布尔值
        if (data.is_armed !== undefined) {
          telemetry.armed = data.is_armed === true
        } else if (data.arm_status !== undefined) {
          // 兼容旧的 arm_status 字符串
          telemetry.armed = data.arm_status === "解锁"
        } else if (data.base_mode !== undefined) {
          // 兼容更旧的位掩码
          let baseMode = extractNumber(data.base_mode)
          if (baseMode !== null) telemetry.armed = (baseMode & 0x80) !== 0
        }

        // 2. 模式名称：优先使用 flight_mode，其次 mode_type，最后 custom_mode 映射
        if (data.flight_mode !== undefined) {
          telemetry.mode = extractString(data.flight_mode, "未知")
        } else if (data.mode_type !== undefined) {
          telemetry.mode = extractString(data.mode_type, "未知")
        } else if (data.custom_mode !== undefined) {
          let modeVal = extractNumber(data.custom_mode)
          // 简单映射一些常见模式（可根据需要扩展）
          const simpleModeMap = {
            0: "MANUAL",
            1: "ACRO",
            2: "ALT_HOLD",
            3: "AUTO",
            4: "GUIDED",
            5: "LOITER",
            6: "RTL",
            9: "LAND",
            16: "POSHOLD"
          }
          if (modeVal !== null && simpleModeMap[modeVal]) {
            telemetry.mode = simpleModeMap[modeVal]
          } else {
            telemetry.mode = modeVal !== null ? `模式${modeVal}` : "未知"
          }
        }
        break
      case "EKF_STATUS_REPORT":
        telemetry.ekf_flags = extractNumber(data.flags)
        break
      case "VIBRATION":
        telemetry.vib.x = extractNumber(data.vibration_x) ?? 0
        telemetry.vib.y = extractNumber(data.vibration_y) ?? 0
        telemetry.vib.z = extractNumber(data.vibration_z) ?? 0
        break
      case "SCALED_PRESSURE":
        telemetry.press_abs = extractNumber(data.press_abs)
        telemetry.temperature = extractNumber(data.temperature)
        break
      case "MISSION_CURRENT":
        telemetry.mission_seq = extractNumber(data.seq) ?? 0
        break
      case "MISSION_COUNT":
        telemetry.mission_total = extractNumber(data.count) ?? 0
        break
      case "MISSION_ITEM_INT":
        if (data.seq !== undefined && data.command !== undefined) {
          telemetry.current_cmd = `cmd:${extractNumber(data.command)} seq:${extractNumber(data.seq)}`
        }
        break
      default:
        break
    }
    updateLastUpdateTime()
  } catch (e) {
    console.warn("遥测解析错误", e, payloadStr)
  }
}

// ========== 飞控控制指令发送 ==========
function sendCommand(commandObj: any): boolean {
  if (!client || !client.connected) {
    console.warn("MQTT 未连接，无法发送指令")
    alert("MQTT 未连接，请检查网络")
    return false
  }
  const payload = JSON.stringify(commandObj)
  client.publish(TOPIC_SEND, payload, { qos: 1 }, (err) => {
    if (err) console.error("发送指令失败", err)
    else console.log("指令已发送", commandObj)
  })
  return true
}

function arm() {
  sendCommand({ type: "arm", arm: true })
}
function disarm() {
  sendCommand({ type: "arm", arm: false })
}
function setMode(modeName) {
  sendCommand({ type: "set_mode", mode: modeName })
}

// ========== 航线读取逻辑（空闲超时） ==========
let idleTimer: NodeJS.Timeout | null = null

function updateWaypointStats() {
  const elapsed = (Date.now() - waypointsStartTime.value) / 1000
  const received = waypointsMap.value.size
  waypointStats.elapsed = elapsed
  waypointStats.received = received
  waypointStats.rate = elapsed > 0 && received > 0 ? received / elapsed : 0
}

function renderWaypointTable() {
  if (waypointsMap.value.size === 0) {
    waypointTbody.innerHTML = '<tr><td colspan="4" style="text-align:center;">暂无数据，点击“读取航线”</td></tr>'
    return
  }
  const sorted = Array.from(waypointsMap.values()).sort((a, b) => a.seq - b.seq)
  let html = ""
  for (let wp of sorted) {
    html += `<tr>
                        <td>${wp.seq}</td>
                        <td>${wp.lat.toFixed(6)}</td>
                        <td>${wp.lon.toFixed(6)}</td>
                        <td>${wp.alt.toFixed(2)}</td>
                     </tr>`
  }
  waypointTbody.innerHTML = html
  if (tableContainer) tableContainer.scrollTop = tableContainer.scrollHeight

  const elapsed = (Date.now() - waypointsStartTime) / 1000
  const received = waypointsMap.size
  waypointCountSpan.innerText = received
  waypointTimeSpan.innerText = elapsed.toFixed(2)
  if (elapsed > 0 && received > 0) {
    waypointRateSpan.innerText = (received / elapsed).toFixed(2)
  }
  if (expectedTotal > 0) waypointTotalSpan.innerText = expectedTotal
  else waypointTotalSpan.innerText = received
}

function resetIdleTimer() {
  if (!isLoadingWaypoints.value) return
  if (idleTimer) clearTimeout(idleTimer)
  idleTimer = setTimeout(() => {
    if (isLoadingWaypoints.value) {
      finishWaypointLoading(`⏱️ 空闲超时 (5秒无新航点)，共接收 ${waypointsMap.value.size} 个航点`, true)
    }
  }, IDLE_TIMEOUT_MS)
}

function finishWaypointLoading(message: string, isError = false) {
  if (!isLoadingWaypoints.value) return
  isLoadingWaypoints.value = false
  if (idleTimer) {
    clearTimeout(idleTimer)
    idleTimer = null
  }
  waypointStats.status = message
  updateWaypointStats()
}

function handleWaypointMessage(msg: any) {
  if (!isLoadingWaypoints.value) return
  if (msg.type !== "waypoint") return

  if (msg.total_count !== undefined && msg.total_count > 0 && expectedTotal.value === 0) {
    expectedTotal.value = msg.total_count
  }
  const wp = msg.data
  if (wp && typeof wp.seq === "number" && typeof wp.lat === "number" && typeof wp.lon === "number") {
    waypointsMap.value.set(wp.seq, {
      seq: wp.seq,
      lat: wp.lat,
      lon: wp.lon,
      alt: wp.alt
    })
    updateWaypointStats()
    resetIdleTimer()

    if (expectedTotal.value > 0 && waypointsMap.value.size >= expectedTotal.value) {
      finishWaypointLoading(`✅ 完成！共 ${expectedTotal.value} 个航点`, false)
    }
  }
}

function startFetchWaypoints() {
  if (isLoadingWaypoints.value) {
    waypointStats.status = "正在读取中，请稍后或点击停止"
    return
  }
  isLoadingWaypoints.value = true
  waypointsMap.value.clear()
  expectedTotal.value = 0
  waypointsStartTime.value = Date.now()
  waypointStats.status = "⏳ 正在请求航线数据..."
  updateWaypointStats()
  resetIdleTimer()

  if (client && client.connected) {
    const requestMsg = JSON.stringify({ type: "get_list" })
    client.publish(TOPIC_SEND, requestMsg, { qos: 1 }, (err) => {
      if (err) {
        console.error("发布请求失败", err)
        finishWaypointLoading("❌ 发送请求失败", true)
      } else {
        console.log("已发送 get_list 请求")
      }
    })
  } else {
    finishWaypointLoading("❌ MQTT 未连接", true)
  }
}

function stopFetchWaypoints() {
  if (!isLoadingWaypoints.value) return
  finishWaypointLoading(`⏹️ 已手动停止，共接收 ${waypointsMap.value.size} 个航点`, false)
}

// ========== MQTT 连接 ==========
let client: MqttClient | null = null
function connectMQTT() {
  const options = {
    keepalive: 60,
    reconnectPeriod: 3000,
    connectTimeout: 10000,
    clientId: `mav_dash_${Math.random().toString(16).substr(2, 8)}`
  }
  // 如需认证，取消注释
  // options.username = "your_username";
  // options.password = "your_password";

  client = mqtt.connect(BROKER, options)
  client.on("connect", () => {
    connectionStatus.value = "online"
    client.subscribe(TOPIC_MAVLINK, { qos: 1 })
    client.subscribe(TOPIC_GET_LIST, { qos: 1 })
    console.log(`已订阅 ${TOPIC_MAVLINK} 和 ${TOPIC_GET_LIST}`)
  })
  client.on("message", (topic, message) => {
    const payload = message.toString()
    if (topic === TOPIC_MAVLINK) {
      processMavlinkMessage(payload)
    } else if (topic === TOPIC_GET_LIST) {
      try {
        const obj = JSON.parse(payload)
        handleWaypointMessage(obj)
      } catch (e) {
        console.warn("航线消息解析失败", e, payload)
      }
    }
  })
  client.on("error", (err) => {
    console.error(err)
    connectionStatus.value = "error"
  })
  client.on("close", () => {
    connectionStatus.value = "offline"
  })
  client.on("reconnect", () => {
    connectionStatus.value = "connecting"
  })
}

// 组件挂载时初始化
onMounted(() => {
  connectMQTT()
})

// 组件卸载前清理
onBeforeUnmount(() => {
  if (client && client.connected) {
    client.end(true)
  }
  if (idleTimer) {
    clearTimeout(idleTimer)
  }
})
</script>

<template>
  <div class="min-h-screen bg-gray-900 text-white p-6">
    <!-- 头部 -->
    <div class="flex justify-between items-center mb-6">
      <div>
        <h1 class="text-2xl font-bold mb-2">🛩️ MAVLink 遥测 + 航线读取 + 飞控控制</h1>
        <p class="text-gray-400 text-sm">左右布局 | 加锁/解锁 | 模式切换 | 适配后端 HEARTBEAT 结构</p>
      </div>
      <div class="flex items-center gap-4">
        <div class="flex items-center gap-2">
          <div
            :class="[
              'w-3 h-3 rounded-full',
              connectionStatus === 'online'
                ? 'bg-green-500'
                : connectionStatus === 'connecting'
                  ? 'bg-yellow-500'
                  : connectionStatus === 'error'
                    ? 'bg-red-500'
                    : 'bg-gray-500'
            ]"
          ></div>
          <span>{{
            connectionStatus === "online"
              ? "在线"
              : connectionStatus === "connecting"
                ? "连接中"
                : connectionStatus === "error"
                  ? "错误"
                  : "离线"
          }}</span>
        </div>
        <div class="bg-gray-800 px-3 py-1 rounded text-sm">{{ lastUpdateTime }}</div>
      </div>
    </div>

    <!-- 主内容区：左侧遥测 + 右侧控制 -->
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
      <!-- 左侧遥测数据卡片 (保持原有 Tailwind 样式，但内部未用 naive-ui 组件，不影响要求) -->
      <div class="lg:col-span-2">
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <!-- 姿态角 -->
          <div class="bg-gray-800 rounded-lg p-4">
            <div class="text-gray-400 text-sm mb-2">🎯 姿态角</div>
            <div class="text-2xl font-bold mb-1">
              {{ telemetry.roll_deg ?? "---" }}° / {{ telemetry.pitch_deg ?? "---" }}° / {{ telemetry.yaw_deg ?? "---" }}°
            </div>
            <div class="text-gray-500 text-sm">横滚 / 俯仰 / 偏航</div>
          </div>

          <!-- 速度 & 航向 -->
          <div class="bg-gray-800 rounded-lg p-4">
            <div class="text-gray-400 text-sm mb-2">🧭 速度 & 航向</div>
            <div class="text-2xl font-bold mb-1">
              {{ telemetry.groundspeed !== null ? telemetry.groundspeed.toFixed(2) : "---" }} m/s |
              {{ telemetry.heading !== null ? telemetry.heading.toFixed(0) : "---" }}°
            </div>
            <div class="text-gray-500 text-sm">
              空速: {{ telemetry.airspeed !== null ? telemetry.airspeed.toFixed(2) : "---" }} m/s 垂直速度:
              {{ telemetry.climb !== null ? telemetry.climb.toFixed(2) : "---" }} m/s
            </div>
          </div>

          <!-- 位置 & 高度 -->
          <div class="bg-gray-800 rounded-lg p-4">
            <div class="text-gray-400 text-sm mb-2">📍 位置 & 高度</div>
            <div class="text-xl font-bold mb-1">
              {{ telemetry.lat !== null ? (telemetry.lat / 1e7).toFixed(6) : "---" }},
              {{ telemetry.lon !== null ? (telemetry.lon / 1e7).toFixed(6) : "---" }}
            </div>
            <div class="text-gray-500 text-sm">
              相对高度: {{ telemetry.relative_alt !== null ? (telemetry.relative_alt / 1000).toFixed(1) : "---" }} m | 绝对高度:
              {{ telemetry.alt_msl !== null ? (telemetry.alt_msl / 1000).toFixed(1) : "---" }} m
            </div>
          </div>

          <!-- 电源状态 -->
          <div class="bg-gray-800 rounded-lg p-4">
            <div class="text-gray-400 text-sm mb-2">🔋 电源状态</div>
            <div class="text-2xl font-bold mb-1">
              {{ telemetry.voltage !== null ? (telemetry.voltage / 1000).toFixed(1) : "---" }}V |
              {{ telemetry.current !== null ? (telemetry.current / 100).toFixed(1) : "---" }}A
            </div>
            <div class="text-gray-500 text-sm">
              剩余电量: {{ telemetry.battery_remaining !== null ? telemetry.battery_remaining : "---" }}% 消耗:
              {{ telemetry.consumed !== null ? telemetry.consumed : "---" }} mAh
            </div>
          </div>

          <!-- GPS 定位 -->
          <div class="bg-gray-800 rounded-lg p-4">
            <div class="text-gray-400 text-sm mb-2">🛰️ GPS 定位</div>
            <div class="text-xl font-bold mb-1">{{ fixTypeText }}</div>
            <div class="text-gray-500 text-sm">
              🛸 卫星: {{ telemetry.satellites ?? 0 }} | 📡 EPH:
              {{ telemetry.eph !== null ? (telemetry.eph / 100).toFixed(1) : "---" }} m
            </div>
          </div>

          <!-- 系统与EKF -->
          <div class="bg-gray-800 rounded-lg p-4">
            <div class="text-gray-400 text-sm mb-2">⚙️ 系统与EKF</div>
            <div class="text-xl font-bold mb-1">{{ telemetry.armed ? "✅ 已解锁" : "🔒 已上锁" }}</div>
            <div class="text-gray-500 text-sm">飞行模式: {{ telemetry.mode }} | EKF: {{ ekfStatusText }}</div>
          </div>

          <!-- 任务信息 -->
          <div class="bg-gray-800 rounded-lg p-4">
            <div class="text-gray-400 text-sm mb-2">📌 任务信息</div>
            <div class="text-2xl font-bold mb-1">{{ telemetry.mission_seq }} / {{ telemetry.mission_total }} 航点</div>
            <div class="text-gray-500 text-sm">当前指令: {{ telemetry.current_cmd || "--" }}</div>
          </div>

          <!-- 振动监测 -->
          <div class="bg-gray-800 rounded-lg p-4">
            <div class="text-gray-400 text-sm mb-2">📳 振动监测</div>
            <div class="text-lg font-bold mb-1">
              X: {{ telemetry.vib.x.toFixed(3) }} | Y: {{ telemetry.vib.y.toFixed(3) }} | Z: {{ telemetry.vib.z.toFixed(3) }}
            </div>
            <div class="text-gray-500 text-sm">单位: G</div>
          </div>

          <!-- 环境 & 负载 -->
          <div class="bg-gray-800 rounded-lg p-4">
            <div class="text-gray-400 text-sm mb-2">🌡️ 环境 & 负载</div>
            <div class="text-2xl font-bold mb-1">
              {{ telemetry.temperature !== null ? (telemetry.temperature / 100).toFixed(1) : "---" }}°C |
              {{ telemetry.press_abs !== null ? telemetry.press_abs.toFixed(1) : "---" }} hPa
            </div>
            <div class="text-gray-500 text-sm">CPU负载: {{ telemetry.load ?? "---" }}%</div>
          </div>
        </div>
      </div>

      <!-- 右侧：飞控控制 + 航线读取（使用 Naive UI 组件） -->
      <div class="space-y-6">
        <!-- 飞控控制卡片 -->
        <n-card title="🎮 飞控控制" :bordered="false" class="bg-gray-800 text-white shadow-lg">
          <div class="space-y-4">
            <div class="flex gap-2">
              <n-button type="success" @click="arm" strong secondary class="flex-1">🔓 解锁</n-button>
              <n-button type="error" @click="disarm" strong secondary class="flex-1">🔒 加锁</n-button>
            </div>
            <div class="flex gap-2">
              <n-select v-model:value="selectedMode" :options="flightModeOptions" class="flex-1" size="medium" />
              <n-button type="primary" @click="setMode(selectedMode)" class="flex-1">✈️ 切换模式</n-button>
            </div>
            <div class="text-sm text-gray-400">
              当前状态:
              <n-tag :type="telemetry.armed ? 'success' : 'error'" size="small">
                {{ telemetry.armed ? "已解锁" : "已上锁" }}
              </n-tag>
              | 当前模式:
              <n-tag type="info" size="small">{{ telemetry.mode }}</n-tag>
            </div>
          </div>
        </n-card>

        <!-- 航线读取工具卡片 -->
        <n-card title="🗺️ 航线读取工具" :bordered="false" class="bg-gray-800 text-white shadow-lg">
          <div class="space-y-4">
            <div class="flex gap-2">
              <n-button
                type="primary"
                @click="startFetchWaypoints"
                :loading="isLoadingWaypoints"
                :disabled="isLoadingWaypoints"
                class="flex-1"
              >
                📥 读取航线
              </n-button>
              <n-button type="default" @click="stopFetchWaypoints" :disabled="!isLoadingWaypoints" class="flex-1">
                ⏹️ 停止接收
              </n-button>
            </div>
            <div class="grid grid-cols-3 gap-2 text-sm text-gray-400">
              <div>⏱️ 耗时: {{ waypointStats.elapsed.toFixed(2) }}s</div>
              <div>📦 航点: {{ waypointStats.received }} / {{ expectedTotal || waypointStats.received }}</div>
              <div>⚡ 速率: {{ waypointStats.rate.toFixed(2) }}点/秒</div>
            </div>
            <div
              class="text-sm"
              :class="{
                'text-red-400': waypointStats.status.includes('错误'),
                'text-green-400': waypointStats.status.includes('完成'),
                'text-yellow-400': !waypointStats.status.includes('错误') && !waypointStats.status.includes('完成')
              }"
            >
              {{ waypointStats.status || "" }}
            </div>
          </div>

          <!-- 航点表格 (Naive UI Data Table) -->
          <n-data-table
            :columns="waypointColumns"
            :data="waypoints"
            :pagination="false"
            :bordered="false"
            class="mt-4"
            :row-key="(row) => row.seq"
            size="small"
          />
          <div v-if="waypoints.length === 0" class="text-center text-gray-400 py-4">暂无数据，点击“读取航线”</div>
        </n-card>
      </div>
    </div>

    <!-- 页脚 -->
    <div class="mt-6 text-center text-gray-500 text-sm">
      空闲超时: 5秒内无新航点自动结束 | 加锁/解锁/模式切换通过 MQTT send 频道 | 实时显示状态
    </div>
  </div>
</template>
