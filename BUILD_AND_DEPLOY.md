# 构建与部署指南

本指南介绍如何构建和部署 MAVLink 消息转发系统，包括开发环境配置、编译优化、部署流程等。

## 开发环境配置

### 系统要求

- Rust 1.70 或更高版本
- Cargo（Rust 包管理器）
- Git（用于版本控制）

### 安装 Rust

在 Windows 系统上，使用 rustup 安装 Rust：

```bash
# 下载并运行 rustup 安装程序
# 访问 https://rustup.rs/ 获取最新版本

# 验证安装
rustc --version
cargo --version
```

### 克隆项目

```bash
git clone <repository-url>
cd mavlink
```

## 项目依赖

### 主要依赖

项目使用以下主要依赖：

- `actix-web`：Web 框架
- `tokio`：异步运行时
- `mavlink`：MAVLink 协议处理
- `rumqttc`：MQTT 客户端
- `serde` 和 `serde_json`：序列化/反序列化
- `log` 和 `log4rs`：日志记录

### 安装依赖

```bash
# 安装项目依赖
cargo build

# 或者仅下载依赖而不编译
cargo fetch
```

## 本地开发

### 运行开发版本

```bash
# 运行项目（开发模式）
cargo run

# 或者先编译再运行
cargo build
cargo run
```

### 运行测试

```bash
# 运行所有测试
cargo test

# 运行特定测试
cargo test test_name

# 显示测试输出
cargo test -- --nocapture
```

### 代码检查

```bash
# 运行 Clippy 进行代码检查
cargo clippy

# 检查代码格式
cargo fmt --check

# 自动格式化代码
cargo fmt
```

## 生产构建

### 基本构建

```bash
# 构建 Release 版本
cargo build --release
```

### 优化构建选项

在 `Cargo.toml` 中添加以下配置可以减小最终二进制文件的大小：

```toml
[profile.release]
opt-level = "z"     # 优化二进制文件大小
lto = true          # 启用链接时优化
codegen-units = 1   # 减少代码生成单元以提高优化效果
strip = true        # 去除符号信息
```

### 交叉编译

如果需要为其他平台构建，请参考 [CROSS_COMPILE.md](./CROSS_COMPILE.md) 了解如何使用 `cargo-zigbuild` 进行交叉编译。

```bash
# 添加目标平台
rustup target add aarch64-unknown-linux-gnu

# 使用 cargo-zigbuild 进行交叉编译
cargo zigbuild --target aarch64-unknown-linux-gnu --release
```

## 部署

### 部署前准备

1. 确保目标系统满足运行要求
2. 准备配置文件（如有）
3. 确认网络连接和防火墙设置

### 部署步骤

1. **传输二进制文件**

```bash
# 使用 scp 传输文件到目标服务器
scp target/release/my-rust-server user@server:/path/to/deploy/
```

2. **设置执行权限**

```bash
chmod +x /path/to/deploy/my-rust-server
```

3. **创建日志目录**

```bash
mkdir -p /path/to/deploy/logs
```

4. **运行服务**

```bash
# 直接运行
./my-rust-server

# 或者使用 nohup 在后台运行
nohup ./my-rust-server > /path/to/deploy/logs/output.log 2>&1 &

# 或者使用 systemd 管理服务（见下文）
```

### Systemd 服务配置

创建 systemd 服务文件 `/etc/systemd/system/mavlink-forwarder.service`：

```ini
[Unit]
Description=MAVLink Message Forwarder
After=network.target

[Service]
Type=simple
User=mavlink
WorkingDirectory=/path/to/deploy
ExecStart=/path/to/deploy/my-rust-server
Restart=always
RestartSec=5
StandardOutput=append:/path/to/deploy/logs/output.log
StandardError=append:/path/to/deploy/logs/error.log

[Install]
WantedBy=multi-user.target
```

启用和启动服务：

```bash
# 重新加载 systemd 配置
sudo systemctl daemon-reload

# 启用服务（开机自启）
sudo systemctl enable mavlink-forwarder

# 启动服务
sudo systemctl start mavlink-forwarder

# 查看服务状态
sudo systemctl status mavlink-forwarder

# 查看服务日志
sudo journalctl -u mavlink-forwarder -f
```

## 配置管理

### 环境变量

可以通过环境变量配置部分参数：

```bash
# 设置 MQTT 服务器地址
export MQTT_HOST=mqtt.example.com

# 设置 MAVLink 连接字符串
export MAVLINK_CONN=udpin:127.0.0.1:23445

# 运行服务
./my-rust-server
```

### 配置文件

如果项目支持配置文件，可以创建配置文件 `config.toml`：

```toml
[mqtt]
host = "mqtt.example.com"
port = 1883

[mavlink]
connection = "udpin:127.0.0.1:23445"

[logging]
level = "debug"
file = "logs/log.log"
```

## 监控与维护

### 日志管理

项目使用 `log4rs` 进行日志记录，支持日志文件滚动：

- 日志文件位置：`logs/log.log`
- 日志滚动策略：按大小滚动（1MB）
- 保留历史日志：最多 30 个文件

查看日志：

```bash
# 查看最新日志
tail -f logs/log.log

# 查看错误日志
grep ERROR logs/log.log

# 查看特定时间段的日志
grep "2024-01-01 10:" logs/log.log
```

### 性能监控

可以使用系统工具监控服务性能：

```bash
# 查看进程资源使用情况
top -p $(pgrep my-rust-server)

# 查看内存使用情况
ps aux | grep my-rust-server

# 查看网络连接
netstat -an | grep 1883
```

### 健康检查

可以定期检查服务健康状态：

```bash
# 检查服务是否运行
systemctl is-active mavlink-forwarder

# 检查 MQTT 连接
# 可以通过检查日志或实现健康检查端点
```

## 故障排除

### 常见问题

1. **服务无法启动**

   - 检查日志文件查看错误信息
   - 确认依赖的服务（如 MQTT 服务器）是否正常运行
   - 检查端口是否被占用

2. **MAVLink 连接失败**

   - 确认连接字符串是否正确
   - 检查网络连接和防火墙设置
   - 确认飞控设备是否正常工作

3. **MQTT 连接失败**

   - 确认 MQTT 服务器地址和端口是否正确
   - 检查网络连接
   - 确认 MQTT 服务器是否正常运行

4. **性能问题**

   - 检查系统资源使用情况
   - 查看日志是否有错误或警告
   - 考虑优化编译选项或调整配置

### 日志分析

```bash
# 统计错误数量
grep -c ERROR logs/log.log

# 查找特定错误
grep "connection failed" logs/log.log

# 分析错误趋势
grep ERROR logs/log.log | awk '{print $1, $2}' | sort | uniq -c
```

## 更新与升级

### 更新流程

1. 备份当前版本和配置
2. 停止服务
3. 替换二进制文件
4. 更新配置（如有需要）
5. 启动服务
6. 验证功能

```bash
# 停止服务
sudo systemctl stop mavlink-forwarder

# 备份当前版本
cp /path/to/deploy/my-rust-server /path/to/deploy/my-rust-server.bak

# 替换二进制文件
scp target/release/my-rust-server user@server:/path/to/deploy/

# 启动服务
sudo systemctl start mavlink-forwarder

# 验证服务状态
sudo systemctl status mavlink-forwarder
```

### 回滚

如果新版本出现问题，可以快速回滚：

```bash
# 停止服务
sudo systemctl stop mavlink-forwarder

# 恢复备份
cp /path/to/deploy/my-rust-server.bak /path/to/deploy/my-rust-server

# 启动服务
sudo systemctl start mavlink-forwarder
```

## 安全建议

1. **最小权限原则**

   - 使用专用用户运行服务
   - 限制文件系统访问权限
   - 限制网络访问

2. **网络安全**

   - 使用 TLS 加密连接
   - 实现防火墙规则
   - 限制远程访问

3. **日志安全**

   - 避免在日志中记录敏感信息
   - 定期清理旧日志
   - 保护日志文件访问权限

4. **更新维护**

   - 定期更新依赖库
   - 及时应用安全补丁
   - 监控安全公告
