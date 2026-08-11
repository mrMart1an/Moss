use anyhow::anyhow;
use tokio::sync::{mpsc::Sender, oneshot};
use zbus::interface;

use crate::{config_manager::{ConfigMessage, ConfigMessageAnswer}, dbus_service::{dbus_opt, opt_dbus, Result}, extract_answer, gpu_device::gpu_config::GpuConfig};

pub struct ProfileNvidiaInterface {
    profile_name: String,
    tx_config_manager: Sender<ConfigMessage>,
}

#[interface(name = "com.github.Mossd1.NvidiaProfile")]
impl ProfileNvidiaInterface {
    #[zbus(property)]
    async fn core_clock_offset(&self) -> zbus::fdo::Result<(bool, i32)> {
        let config = self
            .get_device_config()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        Ok(opt_dbus(config.nvidia_config.core_clock_offset))
    }
    #[zbus(property)]
    async fn memory_clock_offset(&self) -> zbus::fdo::Result<(bool, i32)> {
        let config = self
            .get_device_config()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        Ok(opt_dbus(config.nvidia_config.mem_clock_offset))
    }

    // Setters
    #[zbus(property)]
    async fn set_core_clock_offset(
        &self,
        core_clock_offset: (bool, i32),
    ) -> zbus::Result<()> {
        let core_offset = dbus_opt(core_clock_offset);

        // Get the current config
        let mut config = self
            .get_device_config()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        // Update it
        config.nvidia_config.core_clock_offset = core_offset;

        // Now set the new config
        self.set_device_config(config)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        Ok(())
    }
    // Setters
    #[zbus(property)]
    async fn set_memory_clock_offset(
        &self,
        memory_clock_offset: (bool, i32),
    ) -> zbus::Result<()> {
        let memory_offset = dbus_opt(memory_clock_offset);

        // Get the current config
        let mut config = self
            .get_device_config()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        // Update it
        config.nvidia_config.mem_clock_offset = memory_offset;

        // Now set the new config
        self.set_device_config(config)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        Ok(())
    }
}

impl ProfileNvidiaInterface {
    pub fn new(
        profile_name: String,
        tx_config_manager: Sender<ConfigMessage>,
    ) -> Self {
        Self {
            profile_name,
            tx_config_manager,
        }
    }

    // Return the current profile GPU config
    async fn get_device_config(&self) -> Result<GpuConfig> {
        let (tx, rx) = oneshot::channel();
        let message = ConfigMessage::GetProfileConfig {
            profile: self.profile_name.clone(),
            tx,
        };

        self.tx_config_manager.send(message).await?;
        let config_answer = rx.await?;

        let config =
            extract_answer!(ConfigMessageAnswer::DeviceConfig, config_answer)?;

        Ok(config)
    }

    // Set the given profile GPU config
    async fn set_device_config(&self, config: GpuConfig) -> Result<()> {
        let message = ConfigMessage::SetProfileDeviceConfig {
            profile: self.profile_name.clone(),
            config,
        };

        self.tx_config_manager.send(message).await?;

        Ok(())
    }
}
