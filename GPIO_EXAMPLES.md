# GPIO使用示例

本文档展示如何在项目中使用GPIO模块。

## 基本概念

GPIO模块提供了以下功能：
1. 配置GPIO引脚为输入或输出模式
2. 设置输出引脚的电平（高/低）
3. 读取输入引脚的电平
4. 设置PWM输出（占空比0.0-1.0）

## 在代码中使用GPIO

### 1. 初始化GPIO系统

在main.rs中已经添加了初始化代码：

```rust
// 初始化GPIO
let gpio_manager = gpio::init_gpio()?;
let gpio_sender = gpio_manager.get_sender();
```

### 2. 设置输出引脚电平

```rust
// 设置引脚17为高电平
gpio_manager.set_pin(17, true).await?;

// 设置引脚27为低电平
gpio_manager.set_pin(27, false).await?;
```

### 3. 读取输入引脚电平

```rust
// 读取引脚18的电平
let value = gpio_manager.read_pin(18).await?;
log::info!("引脚18电平: {}", value);
```

### 4. 设置PWM输出

```rust
// 设置引脚18的PWM占空比为50%
gpio_manager.set_pwm(18, 0.5).await?;

// 设置引脚18的PWM占空比为75%
gpio_manager.set_pwm(18, 0.75).await?;
```

## 预配置的GPIO引脚

默认配置了以下GPIO引脚：

| 引脚号 | 名称 | 类型 |
|--------|------|------|
| 17 | LED_RED | 输出 |
| 27 | LED_GREEN | 输出 |
| 18 | BUTTON | 输入 |

## 实际应用示例

### 示例1：LED闪烁

```rust
// 让红色LED闪烁
loop {
    gpio_manager.set_pin(17, true).await?;  // 点亮
    tokio::time::sleep(Duration::from_secs(1)).await;
    gpio_manager.set_pin(17, false).await?; // 熄灭
    tokio::time::sleep(Duration::from_secs(1)).await;
}
```

### 示例2：按钮检测

```rust
// 持续检测按钮状态
loop {
    let button_pressed = gpio_manager.read_pin(18).await?;
    if button_pressed {
        log::info!("按钮被按下");
        gpio_manager.set_pin(27, true).await?; // 点亮绿色LED
    } else {
        gpio_manager.set_pin(27, false).await?; // 熄灭绿色LED
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
}
```

### 示例3：PWM控制

```rust
// 渐变LED亮度
for i in 0..=100 {
    let duty = i as f32 / 100.0;
    gpio_manager.set_pwm(17, duty).await?;
    tokio::time::sleep(Duration::from_millis(20)).await;
}
```

## 注意事项

1. 确保你的硬件平台支持GPIO操作
2. 在实际使用前，需要实现TODO标记的GPIO操作代码
3. PWM功能需要硬件支持
4. 读取引脚电平有1秒超时限制
5. PWM占空比必须在0.0到1.0之间

## 硬件相关实现

要使GPIO功能实际工作，需要根据你的硬件平台实现以下功能：

1. **sysfs方式**（Linux系统）：
   - 通过/sys/class/gpio目录操作GPIO
   - 写入direction文件设置方向
   - 写入value文件设置/读取电平

2. **嵌入式HAL**：
   - 使用embedded-hal crate
   - 实现具体的硬件抽象层

3. **专用库**：
   - Linux: rppal (树莓派)
   - 其他平台: 查找相应的GPIO库

## 错误处理

所有GPIO操作都返回Result类型，应该正确处理可能的错误：

```rust
match gpio_manager.set_pin(17, true).await {
    Ok(_) => log::info!("成功设置引脚"),
    Err(e) => log::error!("设置引脚失败: {}", e),
}
```
