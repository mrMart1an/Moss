use anyhow::anyhow;
use tokio::sync::{mpsc::Sender, oneshot};
use zbus::{interface, object_server::SignalEmitter};

use crate::{
    devices_manager::{DevicesManagerAnswer, DevicesManagerMessage},
    extract_answer,
    gpu_device::{gpu_data::GpuData, gpu_info::GpuInfo},
};

type Result<T> = std::result::Result<T, anyhow::Error>;

// GPU D-Bus interface
pub struct GpuInterface {
    uuid: String,

    tx_device_manager: Sender<DevicesManagerMessage>,

    gpu_info: GpuInfo,
}

#[interface(name = "com.github.Mossd1.Gpu")]
impl GpuInterface {
    // GPU info properties
    #[zbus(property)]
    async fn uuid(&self) -> &str {
        &self.uuid
    }
    #[zbus(property)]
    async fn name(&self) -> &str {
        &self.gpu_info.name
    }

    #[zbus(property)]
    async fn pcie_width(&self) -> u32 {
        self.gpu_info.pcie_width
    }
    #[zbus(property)]
    async fn pcie_gen(&self) -> u32 {
        self.gpu_info.pcie_gen
    }

    #[zbus(property)]
    async fn power_limit_max(&self) -> u32 {
        self.gpu_info.power_limit_max
    }
    #[zbus(property)]
    async fn power_limit_min(&self) -> u32 {
        self.gpu_info.power_limit_min
    }
    #[zbus(property)]
    async fn power_limit_default(&self) -> u32 {
        self.gpu_info.power_limit_default
    }

    // GPU data properties
    #[zbus(property)]
    async fn temperature(&self) -> zbus::fdo::Result<u32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.temp_gpu)
    }

    #[zbus(property)]
    async fn graphics_frequency(&self) -> zbus::fdo::Result<u32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.graphics_freq)
    }
    #[zbus(property)]
    async fn memory_frequency(&self) -> zbus::fdo::Result<u32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.mem_freq)
    }

    #[zbus(property)]
    async fn core_clock_offset(&self) -> zbus::fdo::Result<i32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.core_clock_offset)
    }
    #[zbus(property)]
    async fn memory_clock_offset(&self) -> zbus::fdo::Result<i32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.mem_clock_offset)
    }

    #[zbus(property)]
    async fn power_usage(&self) -> zbus::fdo::Result<u32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.power_usage)
    }
    #[zbus(property)]
    async fn power_limit(&self) -> zbus::fdo::Result<u32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.power_limit)
    }

    #[zbus(property)]
    async fn fan_speed(&self) -> zbus::fdo::Result<u32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.fan_speed)
    }
    #[zbus(property)]
    async fn fan_speed_rpm(&self) -> zbus::fdo::Result<u32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.fan_speed_rpm)
    }

    #[zbus(property)]
    async fn core_usage(&self) -> zbus::fdo::Result<u32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.core_usage)
    }
    #[zbus(property)]
    async fn memory_usage(&self) -> zbus::fdo::Result<u32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.mem_usage)
    }

    #[zbus(property)]
    async fn total_memory(&self) -> zbus::fdo::Result<u64> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.total_memory)
    }
    #[zbus(property)]
    async fn used_memory(&self) -> zbus::fdo::Result<u64> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.used_memory)
    }
    #[zbus(property)]
    async fn free_memory(&self) -> zbus::fdo::Result<u64> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.free_memory)
    }

    // Fan update signal
    #[zbus(signal)]
    pub async fn fan_update(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

impl GpuInterface {
    pub async fn new(
        uuid: String,
        tx_device_manager: Sender<DevicesManagerMessage>,
    ) -> Result<Self> {
        // Get the GPU infos
        let (tx, rx) = oneshot::channel();
        let message = DevicesManagerMessage::GetDeviceInfo {
            uuid: uuid.clone(),
            tx,
        };

        tx_device_manager.send(message).await?;
        let answer = rx.await?;

        let gpu_info =
            extract_answer!(DevicesManagerAnswer::DeviceInfo, answer)?;

        Ok(Self {
            uuid,
            tx_device_manager,
            gpu_info,
        })
    }

    async fn get_data(&self) -> Result<GpuData> {
        // Get the GPU infos
        let (tx, rx) = oneshot::channel();
        let message = DevicesManagerMessage::GetDeviceData {
            uuid: self.uuid.clone(),
            tx,
        };

        self.tx_device_manager.send(message).await?;
        let answer = rx.await?;

        let gpu_data =
            extract_answer!(DevicesManagerAnswer::DeviceData, answer)?;

        // Return an error if no data was provided by the manager
        if let Some(data) = gpu_data {
            Ok(data.0)
        } else {
            Err(anyhow!("Manager failed to provide device data"))
        }
    }
}
