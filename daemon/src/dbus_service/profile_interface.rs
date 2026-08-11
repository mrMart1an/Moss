use anyhow::anyhow;
use std::time::Duration;

use tokio::sync::{mpsc::Sender, oneshot};
use zbus::interface;

use crate::{
    config_manager::{ConfigMessage, ConfigMessageAnswer},
    dbus_service::{Result, dbus_opt, opt_dbus},
    extract_answer,
    fan_curve::fan_mode::FanMode,
    gpu_device::gpu_config::GpuConfig,
};

pub struct ProfileInterface {
    profile_name: String,
    tx_config_manager: Sender<ConfigMessage>,
}

#[interface(name = "com.github.Mossd1.Profile")]
impl ProfileInterface {
    #[zbus(property)]
    async fn fan_update_interval(&self) -> zbus::fdo::Result<(bool, f64)> {
        let interval = self
            .get_fan_update_interval()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        Ok(opt_dbus(interval))
    }
    #[zbus(property)]
    async fn data_update_interval(&self) -> zbus::fdo::Result<(bool, f64)> {
        let interval = self
            .get_data_update_interval()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        Ok(opt_dbus(interval))
    }

    #[zbus(property)]
    async fn fan_mode(&self) -> zbus::fdo::Result<String> {
        let mode = self
            .get_fan_mode()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        Ok(mode.into())
    }
    #[zbus(property)]
    async fn fan_curve(&self) -> zbus::fdo::Result<(bool, String)> {
        let curve_name = self
            .get_fan_curve_name()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        Ok(opt_dbus(curve_name))
    }

    #[zbus(property)]
    async fn power_limit(&self) -> zbus::fdo::Result<(bool, u32)> {
        let config = self
            .get_device_config()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        Ok(opt_dbus(config.power_limit))
    }

    // Setters
    #[zbus(property)]
    async fn set_fan_update_interval(
        &self,
        interval: (bool, f64),
    ) -> zbus::Result<()> {
        let interval = dbus_opt(interval);

        let update_interval = if let Some(interval_f64) = interval {
            Some(Duration::from_secs_f64(interval_f64))
        } else {
            None
        };

        let message = ConfigMessage::SetProfileFanUpdateInterval {
            profile: self.profile_name.clone(),
            update_interval,
        };
        self.tx_config_manager
            .send(message)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        Ok(())
    }
    #[zbus(property)]
    async fn set_data_update_interval(
        &self,
        interval: (bool, f64),
    ) -> zbus::Result<()> {
        let interval = dbus_opt(interval);

        let update_interval = if let Some(interval_f64) = interval {
            Some(Duration::from_secs_f64(interval_f64))
        } else {
            None
        };

        let message = ConfigMessage::SetProfileDataUpdateInterval {
            profile: self.profile_name.clone(),
            update_interval,
        };
        self.tx_config_manager
            .send(message)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        Ok(())
    }

    #[zbus(property)]
    async fn set_fan_mode(&self, fan_mode: String) -> zbus::Result<()> {
        let fan_mode = FanMode::try_from(fan_mode)
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        let message = ConfigMessage::SetProfileFanMode {
            profile: self.profile_name.clone(),
            mode: fan_mode,
        };
        self.tx_config_manager
            .send(message)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        Ok(())
    }
    #[zbus(property)]
    async fn set_fan_curve(
        &self,
        fan_curve: (bool, String),
    ) -> zbus::Result<()> {
        let fan_curve = dbus_opt(fan_curve);

        let message = ConfigMessage::SetProfileFanCurve {
            profile: self.profile_name.clone(),
            curve_name: fan_curve,
        };
        self.tx_config_manager
            .send(message)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        Ok(())
    }

    #[zbus(property)]
    async fn set_power_limit(
        &self,
        power_limit: (bool, u32),
    ) -> zbus::Result<()> {
        let power_limit = dbus_opt(power_limit);

        // Get the current config
        let mut config = self
            .get_device_config()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        // Update it
        config.power_limit = power_limit;

        // Now set the new config
        self.set_device_config(config)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        Ok(())
    }
}

impl ProfileInterface {
    pub fn new(
        profile_name: String,
        tx_config_manager: Sender<ConfigMessage>,
    ) -> Self {
        Self {
            profile_name,
            tx_config_manager,
        }
    }

    // Return the fan update duration as a f64
    async fn get_fan_update_interval(&self) -> Result<Option<f64>> {
        let (tx, rx) = oneshot::channel();
        let message = ConfigMessage::GetProfileFanUpdateInterval {
            profile: self.profile_name.clone(),
            tx,
        };

        self.tx_config_manager.send(message).await?;
        let interval_answer = rx.await?;

        let interval = extract_answer!(
            ConfigMessageAnswer::FanUpdateInterval,
            interval_answer
        )?;

        let f64_interval = if let Some(int) = interval {
            Some(int.as_secs_f64())
        } else {
            None
        };

        Ok(f64_interval)
    }
    // Return the data update duration as a f64
    async fn get_data_update_interval(&self) -> Result<Option<f64>> {
        let (tx, rx) = oneshot::channel();
        let message = ConfigMessage::GetProfileDataUpdateInterval {
            profile: self.profile_name.clone(),
            tx,
        };

        self.tx_config_manager.send(message).await?;
        let interval_answer = rx.await?;

        let interval = extract_answer!(
            ConfigMessageAnswer::DataUpdateInterval,
            interval_answer
        )?;

        let f64_interval = if let Some(int) = interval {
            Some(int.as_secs_f64())
        } else {
            None
        };

        Ok(f64_interval)
    }

    // Return the current profile fan mode
    async fn get_fan_mode(&self) -> Result<FanMode> {
        let (tx, rx) = oneshot::channel();
        let message = ConfigMessage::GetProfileFanMode {
            profile: self.profile_name.clone(),
            tx,
        };

        self.tx_config_manager.send(message).await?;
        let fan_mode_answer = rx.await?;

        let fan_mode =
            extract_answer!(ConfigMessageAnswer::FanMode, fan_mode_answer)?;

        Ok(fan_mode)
    }
    // Return the current profile fan curve name
    async fn get_fan_curve_name(&self) -> Result<Option<String>> {
        let (tx, rx) = oneshot::channel();
        let message = ConfigMessage::GetProfileFanCurveName {
            profile: self.profile_name.clone(),
            tx,
        };

        self.tx_config_manager.send(message).await?;
        let fan_curve_answer = rx.await?;

        let fan_curve_name = extract_answer!(
            ConfigMessageAnswer::FanCurveName,
            fan_curve_answer
        )?;

        Ok(fan_curve_name)
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
