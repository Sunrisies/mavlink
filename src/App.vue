<script setup lang="ts">
import mqtt from "mqtt"
import { ref, reactive, onMounted, computed, onBeforeUnmount } from "vue"
import * as L from "leaflet"
import type { LatLngExpression } from "leaflet"
let LMap: L.Map | null = null
const init = () => {
  LMap = L.map("leaf_map", {
    center: [30.5217, 114.3948],
    zoom: 13,
    minZoom: 1,
    maxZoom: 21, //限制显示地理范围
    maxBounds: L.latLngBounds(L.latLng(-180, -180), L.latLng(180, 180))
  })

  addTMap(LMap, "img")
  addTMap(LMap, "cva") //定义一个比例尺控件
  var scale = L.control.scale()
  //将比例尺加载到地图容器中
  LMap.addControl(scale)
}
type TMapType = "vec" | "cva" | "img" | "cia" | "ter" | "cta" | "ibo" | "eva" | "eia"

/**
 * 为cesium添加天地图的底图
 * @param map
 * @param layer
 * vec：矢量底图、cva：矢量标注、img：影像底图、cia：影像标注
 * ter：地形晕渲、cta：地形标注、eva：矢量英文标注、eia：影像英文标注
 */
function addTMap(map: L.Map, layer: TMapType) {
  // 添加天地图影像注记底图 d434002ddef854e56c24ce68e885a55b
  const cvaLayer = L.tileLayer(
    `http://t0.tianditu.gov.cn/DataServer?T=${layer}_w&x={x}&y={y}&l={z}&tk=6460c0045a3ab09f33e9f40c2d1d36a4`,
    {
      noWrap: true
    }
  )
  return cvaLayer.addTo(map)
}
onMounted(() => {
  init()
})
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
  roll_deg: null as string | null,
  pitch_deg: null as string | null,
  yaw_deg: null as string | null,
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
interface Waypoint {
  seq: number
  lat: number
  lon: number
  alt: number
}
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

// 飞行模式选项
const flightModeOptions = [
  { label: "手动模式", value: "MANUAL" },
  { label: "定高模式", value: "ALT_HOLD" },
  { label: "悬停模式", value: "HOLD" },
  { label: "自动模式", value: "AUTO" },
  { label: "返航模式", value: "RTL" },
  { label: "降落模式", value: "LAND" },
  { label: "特技模式", value: "ACRO" },
  { label: "位置保持", value: "POSHOLD" },
  { label: "跟随模式", value: "FOLLOW" } // 新增模式
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
        if (data.altitude !== undefined) telemetry.relative_alt = extractNumber(data.altitude)! * 1000
        break
      case "VFR_HUD":
        if (data.airspeed !== undefined) telemetry.airspeed = extractNumber(data.airspeed)
        if (data.groundspeed !== undefined) telemetry.groundspeed = extractNumber(data.groundspeed)
        if (data.heading !== undefined) telemetry.heading = extractNumber(data.heading)
        if (data.climb !== undefined) telemetry.climb = extractNumber(data.climb)
        if (data.alt !== undefined) telemetry.relative_alt = extractNumber(data.alt)! * 1000
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
        if (data.load !== undefined) telemetry.load = extractNumber(data.load)
        break
      case "BATTERY_STATUS":
        if (data.voltages && Array.isArray(data.voltages) && data.voltages[0] !== 65535)
          telemetry.voltage = extractNumber(data.voltages[0])
        if (data.current_battery !== undefined) telemetry.current = extractNumber(data.current_battery)
        if (data.battery_remaining !== undefined) telemetry.battery_remaining = extractNumber(data.battery_remaining)
        if (data.current_consumed !== undefined) telemetry.consumed = extractNumber(data.current_consumed)
        break
      case "GPS_RAW_INT": {
        let latVal = extractNumber(data.lat)
        let lonVal = extractNumber(data.lon)
        if (latVal !== null) telemetry.lat = latVal
        if (lonVal !== null) telemetry.lon = lonVal
        telemetry.fix_type = extractNumber(data.fix_type)
        telemetry.satellites = extractNumber(data.satellites_visible)
        telemetry.eph = extractNumber(data.eph)
        break
      }
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
          } as const
          type SimpleModeMap = typeof simpleModeMap
          type ModeValue = keyof SimpleModeMap
          let modeVal = extractNumber(data.custom_mode) as ModeValue | null
          // 检查 modeVal 是否为 null，并且是否存在于 simpleModeMap 中
          if (modeVal !== null) {
            // 将 modeVal 转换为 ModeValue 类型，并检查是否存在于 simpleModeMap 中
            const modeKey = modeVal as unknown as ModeValue
            if (modeKey in simpleModeMap) {
              telemetry.mode = simpleModeMap[modeKey]
            } else {
              telemetry.mode = `模式${modeVal}`
            }
          } else {
            telemetry.mode = "未知"
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
let rateUpdateTimer: NodeJS.Timeout | null = null

function updateWaypointStats() {
  const elapsed = (Date.now() - waypointsStartTime.value) / 1000
  const received = waypointsMap.value.size
  waypointStats.elapsed = elapsed
  waypointStats.received = received
  waypointStats.rate = elapsed > 0 && received > 0 ? received / elapsed : 0
}

function startRateUpdater() {
  if (rateUpdateTimer) clearInterval(rateUpdateTimer)
  rateUpdateTimer = setInterval(() => {
    if (isLoadingWaypoints.value) {
      updateWaypointStats()
    }
  }, 200)
}

function stopRateUpdater() {
  if (rateUpdateTimer) {
    clearInterval(rateUpdateTimer)
    rateUpdateTimer = null
  }
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
  stopRateUpdater()
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
      console.log(waypointsMap.value, "waypointsMap.value")
      // 从 waypointsMap 中提取航点坐标，过滤掉 key 为 0 的点
      const waypoints = Array.from(waypointsMap.value.entries())
        .filter(([key]) => key !== 0) // 过滤掉 key 为 0 的点
        .map(([key, wp]) => ({
          key,
          lat: wp.lat,
          lon: wp.lon,
          alt: wp.alt
        }))
      if (!LMap) {
        return
      }
      // 创建一个图层组来存放所有航点标记
      const waypointsLayer = L.layerGroup().addTo(LMap)

      // 使用 CircleMarker 代替普通 marker，性能更好
      waypoints.forEach((wp) => {
        const circleMarker = L.circleMarker([wp.lat, wp.lon], {
          radius: 5,
          fillColor: "#ff7800",
          color: "#000",
          weight: 1,
          opacity: 1,
          fillOpacity: 0.8
        }).addTo(waypointsLayer)

        // 添加弹出信息，显示航点序号和高度
        circleMarker.bindPopup(`
      <div>
        <strong>航点 ${wp.key}</strong><br>
        纬度: ${wp.lat.toFixed(6)}<br>
        经度: ${wp.lon.toFixed(6)}<br>
        高度: ${wp.alt.toFixed(2)} m
      </div>
    `)
      })
      console.log(waypoints, "waypoints")
      const waypoints1 = waypoints.map((wp) => [wp.lat, wp.lon]) as LatLngExpression[]
      // 创建并添加航线到地图
      var polyline = L.polyline(waypoints1, { color: "red" }).addTo(LMap!)

      // 缩放地图以适应航线
      LMap!.fitBounds(polyline.getBounds())
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
  waypointStats.status = "正在请求航线数据..."
  updateWaypointStats()
  startRateUpdater()
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

// 存储地图点击事件处理函数
let mapClickHandler: ((e: any) => void) | null = null

// 存储航线图层
let waypointsLayer: L.LayerGroup | null = null
let polylineLayer: L.Polyline | null = null

// 添加航点
const addWaypoints = () => {
  if (!LMap) {
    console.warn("地图未初始化")
    return
  }
  
  // 如果已经在添加模式，则取消添加模式
  if (mapClickHandler) {
    LMap.off("click", mapClickHandler)
    mapClickHandler = null
    waypointStats.status = "已退出添加航点模式"
    return
  }
  
  // 创建图层组（如果不存在）
  if (!waypointsLayer) {
    waypointsLayer = L.layerGroup().addTo(LMap)
  }
  
  // 进入添加航点模式
  waypointStats.status = "点击地图添加航点，再次点击“添加航线”按钮退出"
  
  // 添加地图点击事件
  mapClickHandler = (e: any) => {
    const { lat, lng } = e.latlng
    const seq = waypointsMap.value.size + 1
    const defaultAlt = 100 // 默认高度
    
    // 添加航点到Map
    waypointsMap.value.set(seq, {
      seq: seq,
      lat: lat,
      lon: lng,
      alt: defaultAlt
    })
    
    // 在地图上添加航点标记
    const circleMarker = L.circleMarker([lat, lng], {
      radius: 5,
      fillColor: "#ff7800",
      color: "#000",
      weight: 1,
      opacity: 1,
      fillOpacity: 0.8
    }).addTo(waypointsLayer!)
    
    // 添加弹出信息
    circleMarker.bindPopup(`
      <div>
        <strong>航点 ${seq}</strong><br>
        纬度: ${lat.toFixed(6)}<br>
        经度: ${lng.toFixed(6)}<br>
        高度: ${defaultAlt.toFixed(2)} m
      </div>
    `)
    
    // 更新航线
    updatePolyline()
    
    // 更新状态
    waypointStats.status = `已添加第 ${seq} 个航点，继续点击地图添加更多航点，或点击“添加航线”按钮退出`
    updateWaypointStats()
  }
  
  // 绑定点击事件
  LMap.on("click", mapClickHandler)
}

// 更新航线
const updatePolyline = () => {
  if (!LMap || !waypointsLayer) return
  
  // 移除旧的航线
  if (polylineLayer) {
    waypointsLayer.removeLayer(polylineLayer)
  }
  
  // 获取所有航点并按序号排序
  const waypoints = Array.from(waypointsMap.value.values())
    .sort((a, b) => a.seq - b.seq)
  
  // 如果有足够的航点，绘制航线
  if (waypoints.length >= 2) {
    const waypointsCoords = waypoints.map(wp => [wp.lat, wp.lon]) as LatLngExpression[]
    polylineLayer = L.polyline(waypointsCoords, { color: "red" }).addTo(waypointsLayer)
  }
}

// 发送航点到飞控
const sendWaypoints = () => {
  if (waypointsMap.value.size === 0) {
    alert("没有航点可发送")
    return
  }
  
  // 将航点转换为数组格式
  const waypoints = Array.from(waypointsMap.value.values())
    .sort((a, b) => a.seq - b.seq)
    .map(wp => ({
      seq: wp.seq,
      lat: wp.lat,
      lon: wp.lon,
      alt: wp.alt
    }))
  
  // 添加seq为0的起始点
  const waypointData = [
    {
      seq: 0,
      lat: 0,
      lon: 0,
      alt: 0
    },
    ...waypoints
  ]
  
  // 发送航点数据
  sendCommand({
    type: "set_list",
    data: waypointData
  })
  
  waypointStats.status = `✅ 已发送 ${waypoints.length} 个航点到飞控`
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
const telemetryType = {
  MANUAL: "手动",
  HOLD: "停船",
  AUTO: "自动",
  GUIDE: "指点",
  RTL: "返航",
  LOITER: "留待"
} as const
</script>

<template>
  <div class="bg-black-300 h-full w-full relative">
    <div id="leaf_map" class="h-full w-full"></div>
    <div class="w-125 px-2 py-3 flex flex-col absolute z-999 top-2 right-2 border contrast">
      <div class="flex justify-between">
        <div class="flex items-center gap-2" style="display: flex; align-items: center; gap: 8px">
          <span
            class="w-2 h-2 rounded-full"
            :style="{
              background: connectionStatus === 'online' ? '#0bdf50' : connectionStatus === 'connecting' ? '#ff5600' : '#7b7b78'
            }"
          ></span>
          <span style="color: #626260; font-size: 14px">
            {{ connectionStatus === "online" ? "已连接" : connectionStatus === "connecting" ? "连接中" : "未连接" }}
          </span>
        </div>
      </div>

      <!-- 主内容 -->
      <div class="flex-1">
        <!-- 左侧数据区 - 紧凑布局 -->
        <div>
          <div class="grid gap-3 text-base text-white font-semibold" style="grid-template-columns: repeat(4, 1fr)">
            <!-- 横滚 -->
            <div class="border border-gray-400 flex gap-2 h-14 items-center px-3">
              <span>横滚</span>
              <div class="ml-auto text-sm" style="font-family: Saans, ui-sans-serif">{{ telemetry.roll_deg ?? "--" }}°</div>
            </div>

            <!-- 俯仰 -->
            <div class="border border-gray-300 flex gap-2 h-14 items-center px-3">
              <span>俯仰</span>
              <div class="ml-auto text-sm" style="font-family: Saans, ui-sans-serif">{{ telemetry.pitch_deg ?? "--" }}°</div>
            </div>

            <!-- 偏航 -->
            <div class="border border-gray-300 flex gap-2 h-14 items-center px-3">
              <span>偏航</span>
              <div class="ml-auto text-sm" style="font-family: Saans, ui-sans-serif">{{ telemetry.yaw_deg ?? "--" }}°</div>
            </div>

            <!-- 航向 -->
            <div class="border border-gray-300 flex gap-2 h-14 items-center px-3">
              <span>航向</span>
              <div class="ml-auto text-sm" style="font-family: Saans, ui-sans-serif">
                {{ telemetry.heading?.toFixed(0) ?? "--" }}°
              </div>
            </div>

            <!-- 地速 -->
            <div class="border border-gray-300 flex gap-2 h-14 items-center px-3">
              <span>地速</span>
              <div class="ml-auto text-sm" style="font-family: Saans, ui-sans-serif">
                {{ telemetry.groundspeed?.toFixed(1) ?? "--" }}
              </div>
            </div>

            <!-- 相对高度 -->
            <div class="border border-gray-300 flex gap-2 h-14 items-center px-3">
              <span>高度</span>
              <div class="ml-auto text-sm" style="font-family: Saans, ui-sans-serif">
                {{ telemetry.relative_alt ? (telemetry.relative_alt / 1000).toFixed(1) : "--" }}
              </div>
            </div>

            <!-- 电池电压 -->
            <div class="border border-gray-300 flex gap-2 h-14 items-center px-3">
              <span>电池</span>
              <div class="ml-auto text-sm" style="font-family: Saans, ui-sans-serif">
                {{ telemetry.voltage ? (telemetry.voltage / 1000).toFixed(1) : "--" }}
              </div>
            </div>

            <!-- 电量 -->
            <div class="border border-gray-300 flex gap-2 h-14 items-center px-3">
              <span>电量</span>
              <div class="ml-auto text-sm" style="font-family: Saans, ui-sans-serif">
                {{ telemetry.battery_remaining ?? "--" }}%
              </div>
            </div>
            <div class="border border-gray-300 flex flex-col justify-center h-14 px-3">
              <div class="flex items-center gap-2">
                <span>GPS定位</span>
                <div class="ml-auto text-sm font-bold" style="font-family: Saans, ui-sans-serif">{{ fixTypeText }}</div>
              </div>
            </div>

            <!-- GPS状态 -->
            <div class="border border-gray-300 flex flex-col justify-center h-14 px-3">
              <div class="text-sm text-gray-500 mt-1">
                {{ telemetry.eph !== null ? (telemetry.eph / 100).toFixed(1) : "---" }} m
              </div>
            </div>

            <!-- 卫星数 -->
            <div class="border border-gray-300 flex gap-2 h-14 items-center px-3">
              <span>卫星</span>
              <div class="ml-auto text-sm" style="font-family: Saans, ui-sans-serif">{{ telemetry.satellites ?? 0 }}</div>
            </div>

            <!-- 锁定状态 -->
            <div class="border border-gray-300 flex gap-2 h-14 items-center px-3">
              <span>锁定</span>
              <div
                class="ml-auto text-sm"
                :style="{ color: telemetry.armed ? '#0bdf50' : '#fe4c02', fontFamily: 'Saans, ui-sans-serif' }"
              >
                {{ telemetry.armed ? "已解锁" : "已锁定" }}
              </div>
            </div>

            <!-- 飞行模式 -->
            <div class="border border-gray-300 flex gap-2 h-14 items-center px-3">
              <span>模式</span>
              <div class="ml-auto text-sm" style="font-family: Saans, ui-sans-serif">{{ telemetryType[telemetry.mode] }}</div>
            </div>

            <!-- 位置坐标 -->
            <div class="border border-gray-300 flex flex-col justify-center h-14 px-3">
              <div class="text-sm font-mono" style="font-family: ui-monospace">
                {{ telemetry.lat ? (telemetry.lat / 1e7).toFixed(6) : "--" }},
                {{ telemetry.lon ? (telemetry.lon / 1e7).toFixed(6) : "--" }}
              </div>
            </div>
          </div>
        </div>

        <!-- 右侧控制区 - 加宽 -->
        <div class="flex-1 py-2 px-1">
          <!-- 解锁/加锁 -->
          <div class="flex items-center gap-2">
            <div class="text-xs uppercase mb-3" style="color: #7b7b78; letter-spacing: 0.6px; font-family: ui-monospace">
              飞控控制
            </div>
            <div class="flex gap-2">
              <n-button @click="arm" type="primary"> 解锁 </n-button>
              <n-button @click="disarm" type="error"> 加锁 </n-button>
            </div>
            <div class="flex items-center justify-between mb-4 border-red-400">
              <div class="text-xs font-semibold uppercase tracking-wider" style="color: #6b7280; font-family: ui-monospace">
                飞行模式
              </div>
              <n-select v-model:value="selectedMode" :options="flightModeOptions" />
              <div class="absolute right-3 top-1/2 -translate-y-1/2 pointer-events-none">
                <svg class="w-4 h-4 text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"></path>
                </svg>
              </div>
              <n-button @click="setMode(selectedMode)" type="primary"> 切换模式 </n-button>
            </div>
          </div>

          <div class="">
            <div class="text-xs uppercase mb-3" style="color: #7b7b78; letter-spacing: 0.6px; font-family: ui-monospace">
              航线读取
            </div>
            <div class="flex gap-2 mb-3">
              <n-button @click="startFetchWaypoints" type="primary" :disabled="isLoadingWaypoints">
                {{ isLoadingWaypoints ? "读取中" : "读取航线" }}
              </n-button>
              <n-button @click="stopFetchWaypoints" type="primary" :disabled="!isLoadingWaypoints"> 停止 </n-button>
              <n-button @click="addWaypoints" type="primary"> 添加航线 </n-button>
              <n-button @click="sendWaypoints" type="success"> 发送 </n-button>
            </div>
            <div class="grid grid-cols-3 gap-2 mb-2 border border-gray-50 text-white">
              <div class="text-center p-2 rounded">
                <div class="text-xs uppercase" style="color: #7b7b78; letter-spacing: 0.6px; font-family: ui-monospace">
                  已读取
                </div>
                <div class="text-sm font-semibold">{{ waypointStats.received }}</div>
              </div>
              <div class="text-center p-2 rounded">
                <div class="text-xs uppercase" style="color: #7b7b78; letter-spacing: 0.6px; font-family: ui-monospace">耗时</div>
                <div class="text-sm font-semibold" style="font-family: Saans, ui-sans-serif">
                  {{ waypointStats.elapsed.toFixed(1) }}s
                </div>
              </div>
              <div class="text-center p-2 rounded">
                <div class="text-xs uppercase" style="color: #7b7b78; letter-spacing: 0.6px; font-family: ui-monospace">速度</div>
                <div class="text-sm font-semibold" style="font-family: Saans, ui-sans-serif; color: #ff5600">
                  {{ waypointStats.rate.toFixed(1) }}/s
                </div>
              </div>
            </div>
            <div v-if="waypointStats.status" style="color: #7b7b78; font-size: 12px">{{ waypointStats.status }}</div>
          </div>

          <div class="rounded-lg">
            <div class="text-xs uppercase mb-3" style="color: #7b7b78; letter-spacing: 0.6px; font-family: ui-monospace">
              航点列表
            </div>
            <div class="max-h-84 overflow-y-auto rounded" style="border: 1px solid #dedbd6">
              <table class="w-full text-xs">
                <thead style="background: #faf9f6">
                  <tr>
                    <th
                      class="px-2 py-2 text-left uppercase"
                      style="color: #7b7b78; letter-spacing: 0.6px; font-family: ui-monospace"
                    >
                      #
                    </th>
                    <th
                      class="px-2 py-2 text-left uppercase"
                      style="color: #7b7b78; letter-spacing: 0.6px; font-family: ui-monospace"
                    >
                      纬度
                    </th>
                    <th
                      class="px-2 py-2 text-left uppercase"
                      style="color: #7b7b78; letter-spacing: 0.6px; font-family: ui-monospace"
                    >
                      经度
                    </th>
                    <th
                      class="px-2 py-2 text-left uppercase"
                      style="color: #7b7b78; letter-spacing: 0.6px; font-family: ui-monospace"
                    >
                      高度
                    </th>
                  </tr>
                </thead>
                <tbody style="background: #faf9f6">
                  <tr v-for="wp in waypointsData" :key="wp.key" style="border-top: 1px solid #dedbd6">
                    <td class="px-2 py-2" style="color: #7b7b78">{{ wp.seq }}</td>
                    <td class="px-2 py-2 font-mono" style="color: #7b7b78">{{ wp.lat }}</td>
                    <td class="px-2 py-2 font-mono" style="color: #7b7b78">{{ wp.lon }}</td>
                    <td class="px-2 py-2" style="color: #7b7b78">{{ wp.alt }}m</td>
                  </tr>
                  <tr v-if="waypointsData.length === 0">
                    <td colspan="4" class="px-2 py-4 text-center" style="color: #7b7b78">暂无数据</td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
<style>
.contrast {
  background: rgba(13, 22, 35, 0.9);
  backdrop-filter: blur(12px);
}
</style>
