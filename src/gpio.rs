use anyhow::Result;
use std::time::Duration;
use tokio::sync::mpsc;

/// GPIO引脚配置
#[derive(Debug, Clone)]
pub struct GpioPinConfig {
    pub pin_number: u8,
    pub pin_name: String,
    pub pin_type: GpioPinType,
}

/// GPIO引脚类型
#[derive(Debug, Clone, PartialEq)]
pub enum GpioPinType {
    Input,
    Output,
    Pwm,
}

/// GPIO消息类型
#[derive(Debug)]
pub enum GpioMessage {
    /// 设置引脚电平 (Output)
    SetPin { pin: u8, value: bool },
    /// 读取引脚电平 (Input)
    ReadPin { pin: u8 },
    /// 设置PWM值 (Pwm)
    SetPwm { pin: u8, duty_cycle: f32 },
    /// 读取引脚电平响应
    PinValue { pin: u8, value: bool },
}

/// GPIO管理器
pub struct GpioManager {
    /// GPIO引脚配置列表
    pins: Vec<GpioPinConfig>,
    /// 消息发送器
    tx: mpsc::Sender<GpioMessage>,
}

impl GpioManager {
    /// 创建新的GPIO管理器
    pub fn new(pins: Vec<GpioPinConfig>) -> Self {
        let (tx, mut rx) = mpsc::channel(256);

        // 启动GPIO处理任务
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if let Err(e) = Self::handle_message(msg).await {
                    log::error!("GPIO处理错误: {}", e);
                }
            }
            log::warn!("GPIO管理器任务结束");
        });

        Self { pins, tx }
    }

    /// 获取消息发送器
    pub fn get_sender(&self) -> mpsc::Sender<GpioMessage> {
        self.tx.clone()
    }

    /// 设置输出引脚电平
    pub async fn set_pin(&self, pin: u8, value: bool) -> Result<()> {
        self.tx.send(GpioMessage::SetPin { pin, value }).await?;
        Ok(())
    }

    /// 读取输入引脚电平
    pub async fn read_pin(&self, pin: u8) -> Result<bool> {
        let (resp_tx, mut resp_rx) = mpsc::channel(1);
        // 发送读取请求
        self.tx.send(GpioMessage::ReadPin { pin }).await?;

        // 等待响应（超时1秒）
        tokio::select! {
            Some(GpioMessage::PinValue { pin: p, value }) = resp_rx.recv() => {
                if p == pin {
                    Ok(value)
                } else {
                    Err(anyhow::anyhow!("收到错误的引脚响应"))
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                Err(anyhow::anyhow!("读取引脚超时"))
            }
        }
    }

    /// 设置PWM占空比
    pub async fn set_pwm(&self, pin: u8, duty_cycle: f32) -> Result<()> {
        if !(0.0..=1.0).contains(&duty_cycle) {
            return Err(anyhow::anyhow!("PWM占空比必须在0.0到1.0之间"));
        }
        self.tx.send(GpioMessage::SetPwm { pin, duty_cycle }).await?;
        Ok(())
    }

    /// 处理GPIO消息
    async fn handle_message(msg: GpioMessage) -> Result<()> {
        match msg {
            GpioMessage::SetPin { pin, value } => {
                log::info!("设置GPIO引脚 {} 电平为 {}", pin, value);
                // TODO: 实际的GPIO操作代码
                // 示例：使用sysfs或其他GPIO库
                // gpio::set(pin, value)?;
                Ok(())
            }
            GpioMessage::ReadPin { pin } => {
                log::info!("读取GPIO引脚 {} 电平", pin);
                // TODO: 实际的GPIO读取代码
                // let value = gpio::read(pin)?;
                // 发送响应
                // tx.send(GpioMessage::PinValue { pin, value }).await?;
                Ok(())
            }
            GpioMessage::SetPwm { pin, duty_cycle } => {
                log::info!("设置GPIO引脚 {} PWM占空比为 {}", pin, duty_cycle);
                // TODO: 实际的PWM设置代码
                // pwm::set_duty_cycle(pin, duty_cycle)?;
                Ok(())
            }
            GpioMessage::PinValue { .. } => {
                // 这个消息类型不应该在这里处理
                Ok(())
            }
        }
    }
}

/// 初始化GPIO引脚
pub fn init_gpio() -> Result<GpioManager> {
    // 配置GPIO引脚
    let pins = vec![
        GpioPinConfig {
            pin_number: 17,
            pin_name: "LED_RED".to_string(),
            pin_type: GpioPinType::Output,
        },
        GpioPinConfig {
            pin_number: 27,
            pin_name: "LED_GREEN".to_string(),
            pin_type: GpioPinType::Output,
        },
        GpioPinConfig {
            pin_number: 18,
            pin_name: "BUTTON".to_string(),
            pin_type: GpioPinType::Input,
        },
    ];

    // 创建GPIO管理器
    let manager = GpioManager::new(pins);
    log::info!("GPIO管理器已初始化");

    Ok(manager)
}
