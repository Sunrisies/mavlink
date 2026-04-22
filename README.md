# MAVLink 服务器

## 配置说明

本项目使用 `.env` 文件来管理配置。在首次运行前，请复制 `.env.example` 文件为 `.env` 并根据需要修改配置：

```bash
cp .env.example .env
```

### 配置项说明

- `MAVLINK_CONN_STR`: MAVLink 连接字符串（默认：udpin:127.0.0.1:23445）
- `WEB_HOST`: Web 服务器监听地址（默认：0.0.0.0）
- `WEB_PORT`: Web 服务器端口（默认：8080）
- `MQTT_HOST`: MQTT 服务器地址（默认：mqtt.example.com）
- `MQTT_PORT`: MQTT 服务器端口（默认：1883）
- `MQTT_KEEP_ALIVE`: MQTT 保活时间（秒，默认：5）
- `MQTT_MAX_PACKET_SIZE`: MQTT 最大数据包大小（字节，默认：10485760）
- `MQTT_TOPIC_INCOMING`: MAVLink 消息发布的主题（默认：mavlink/incoming）
- `MQTT_TOPIC_SEND`: 发送控制命令的主题（默认：send）
- `MQTT_TOPIC_GET_LIST`: 获取航点列表的主题（默认：get_list）
- `MQTT_TOPIC_SET_LIST`: 设置航点列表的主题（默认：set_list）

## API 接口

### 获取版本信息

**接口地址**: `GET /api/version`

**返回示例**:

```json
{
  "commits": [
    {
      "author": "xxxx",
      "date": "Mon Apr 6 14:30:00 2026 +0800",
      "email": "xxxxx",
      "hash": "09ffe4272cf0668ef0d93a068056c795a66ff8e7",
      "message": "chore: 添加 .gitignore 和 Cargo.lock 文件"
    }
  ],
  "count": 19,
  "version": "0.1.1"
}
```

## MAVProxy 配置示例

```bash
mavproxy.py --master=/dev/ttyACM0 --baudrate 57600 --out 192.168.31.236:2345 --out 0.0.0.0:14551 --out 0.0.0.0:14550 --out 192.168.31.236:14550 --out 192.168.31.236:14551
```