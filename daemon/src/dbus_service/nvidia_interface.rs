use anyhow::anyhow;
use tokio::sync::{mpsc::Sender, oneshot};
use zbus::interface;

use crate::{
    devices_manager::{DevicesManagerAnswer, DevicesManagerMessage},
    extract_answer,
    gpu_device::{gpu_data::GpuVendorData, gpu_info::GpuVendorInfo},
};

type Result<T> = std::result::Result<T, anyhow::Error>;

pub struct NvidiaInterface {
    uuid: String,

    tx_device_manager: Sender<DevicesManagerMessage>,

    gpu_vendor_info: GpuVendorInfo,
}

#[interface(name = "com.github.Mossd1.Nvidia")]
impl NvidiaInterface {
    // GPU vendor info properties
    #[zbus(property)]
    async fn driver_version(&self) -> &str {
        if let GpuVendorInfo::Nvidia { driver_version, .. } =
            &self.gpu_vendor_info
        {
            driver_version
        } else {
            &"VENDOR INFO NOT NVIDIA!"
        }
    }
    #[zbus(property)]
    async fn vbios(&self) -> &str {
        if let GpuVendorInfo::Nvidia { vbios, .. } = &self.gpu_vendor_info {
            vbios
        } else {
            &"VENDOR INFO NOT NVIDIA!"
        }
    }

    #[zbus(property)]
    async fn cuda_core_count(&self) -> u32 {
        if let GpuVendorInfo::Nvidia {
            cuda_core_count, ..
        } = self.gpu_vendor_info
        {
            cuda_core_count
        } else {
            0
        }
    }

    #[zbus(property)]
    async fn max_temp(&self) -> u32 {
        if let GpuVendorInfo::Nvidia { max_temp, .. } = self.gpu_vendor_info {
            max_temp.unwrap_or(0)
        } else {
            0
        }
    }
    #[zbus(property)]
    async fn mem_max_temp(&self) -> u32 {
        if let GpuVendorInfo::Nvidia { mem_max_temp, .. } = self.gpu_vendor_info
        {
            mem_max_temp.unwrap_or(0)
        } else {
            0
        }
    }
    #[zbus(property)]
    async fn slowdown_temp(&self) -> u32 {
        if let GpuVendorInfo::Nvidia { slowdown_temp, .. } =
            self.gpu_vendor_info
        {
            slowdown_temp.unwrap_or(0)
        } else {
            0
        }
    }
    #[zbus(property)]
    async fn shutdown_temp(&self) -> u32 {
        if let GpuVendorInfo::Nvidia { shutdown_temp, .. } =
            self.gpu_vendor_info
        {
            shutdown_temp.unwrap_or(0)
        } else {
            0
        }
    }

    // Nvidia vendor data
    #[zbus(property)]
    async fn sm_frequency(&self) -> zbus::fdo::Result<u32> {
        let vendor_data = self.get_vendor_data().await.map_err(|e| {
            zbus::fdo::Error::Failed(format!("Failed to retrive data: {}", e))
        })?;

        if let GpuVendorData::Nvidia { sm_freq, .. } = vendor_data {
            Ok(sm_freq.unwrap_or_default())
        } else {
            Err(zbus::fdo::Error::Failed(format!(
                "Wrong vendor data obtain"
            )))
        }
    }
    #[zbus(property)]
    async fn video_frequency(&self) -> zbus::fdo::Result<u32> {
        let vendor_data = self.get_vendor_data().await.map_err(|e| {
            zbus::fdo::Error::Failed(format!("Failed to retrive data: {}", e))
        })?;

        if let GpuVendorData::Nvidia { video_freq, .. } = vendor_data {
            Ok(video_freq.unwrap_or_default())
        } else {
            Err(zbus::fdo::Error::Failed(format!(
                "Wrong vendor data obtain"
            )))
        }
    }
    #[zbus(property)]
    async fn graphic_boost_frequency(&self) -> zbus::fdo::Result<u32> {
        let vendor_data = self.get_vendor_data().await.map_err(|e| {
            zbus::fdo::Error::Failed(format!("Failed to retrive data: {}", e))
        })?;

        if let GpuVendorData::Nvidia {
            graphics_boost_freq,
            ..
        } = vendor_data
        {
            Ok(graphics_boost_freq.unwrap_or_default())
        } else {
            Err(zbus::fdo::Error::Failed(format!(
                "Wrong vendor data obtain"
            )))
        }
    }
    #[zbus(property)]
    async fn memory_boost_frequency(&self) -> zbus::fdo::Result<u32> {
        let vendor_data = self.get_vendor_data().await.map_err(|e| {
            zbus::fdo::Error::Failed(format!("Failed to retrive data: {}", e))
        })?;

        if let GpuVendorData::Nvidia { mem_boost_freq, .. } = vendor_data {
            Ok(mem_boost_freq.unwrap_or_default())
        } else {
            Err(zbus::fdo::Error::Failed(format!(
                "Wrong vendor data obtain"
            )))
        }
    }
    #[zbus(property)]
    async fn sm_boost_frequency(&self) -> zbus::fdo::Result<u32> {
        let vendor_data = self.get_vendor_data().await.map_err(|e| {
            zbus::fdo::Error::Failed(format!("Failed to retrive data: {}", e))
        })?;

        if let GpuVendorData::Nvidia { sm_boost_freq, .. } = vendor_data {
            Ok(sm_boost_freq.unwrap_or_default())
        } else {
            Err(zbus::fdo::Error::Failed(format!(
                "Wrong vendor data obtain"
            )))
        }
    }
    #[zbus(property)]
    async fn video_boost_frequency(&self) -> zbus::fdo::Result<u32> {
        let vendor_data = self.get_vendor_data().await.map_err(|e| {
            zbus::fdo::Error::Failed(format!("Failed to retrive data: {}", e))
        })?;

        if let GpuVendorData::Nvidia {
            video_boost_freq, ..
        } = vendor_data
        {
            Ok(video_boost_freq.unwrap_or_default())
        } else {
            Err(zbus::fdo::Error::Failed(format!(
                "Wrong vendor data obtain"
            )))
        }
    }
}

impl NvidiaInterface {
    pub async fn new(
        uuid: String,
        gpu_vendor_info: GpuVendorInfo,
        tx_device_manager: Sender<DevicesManagerMessage>,
    ) -> Result<Self> {
        Ok(Self {
            uuid,

            tx_device_manager,

            gpu_vendor_info,
        })
    }

    async fn get_vendor_data(&self) -> Result<GpuVendorData> {
        // Get the GPU infos
        let (tx, rx) = oneshot::channel();
        let message = DevicesManagerMessage::GetDeviceVendorData {
            uuid: self.uuid.clone(),
            tx,
        };

        self.tx_device_manager.send(message).await?;
        let answer = rx.await?;

        let gpu_vendor_data =
            extract_answer!(DevicesManagerAnswer::DeviceVendorData, answer)?;

        // Return an error if no data was provided by the manager
        if let Some(data) = gpu_vendor_data {
            Ok(data.0)
        } else {
            Err(anyhow!("Manager failed to provide device data"))
        }
    }
}

