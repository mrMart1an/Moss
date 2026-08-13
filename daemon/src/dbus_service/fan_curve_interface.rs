use anyhow::anyhow;
use tokio::sync::{mpsc::Sender, oneshot};
use zbus::interface;

use crate::{
    config_manager::{ConfigMessage, ConfigMessageAnswer},
    dbus_service::{dbus_opt, opt_dbus},
    extract_answer,
    fan_curve::fan_curve_info::FanCurveInfo,
    gpu_device::Result,
};

pub struct FanCurveInterface {
    curve_name: String,
    tx_config_manager: Sender<ConfigMessage>,
}

#[interface(name = "com.github.Mossd1.FanCurve")]
impl FanCurveInterface {
    #[zbus(property)]
    async fn points(&self) -> zbus::fdo::Result<Vec<(i32, u8)>> {
        let info = self
            .get_fan_curve_info()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        Ok(info.points)
    }

    #[zbus(property)]
    async fn upper_hysteresis(&self) -> zbus::fdo::Result<(bool, u32)> {
        let info = self
            .get_fan_curve_info()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        Ok(opt_dbus(info.upper_threshold))
    }
    #[zbus(property)]
    async fn lower_hysteresis(&self) -> zbus::fdo::Result<(bool, u32)> {
        let info = self
            .get_fan_curve_info()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        Ok(opt_dbus(info.lower_threshold))
    }

    // Setters
    #[zbus(property)]
    async fn set_points(&self, points: Vec<(i32, u8)>) -> zbus::Result<()> {
        let mut info = self
            .get_fan_curve_info()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        // Update the points
        info.points = points;

        self.set_fan_curve_info(info)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        Ok(())
    }

    #[zbus(property)]
    async fn set_upper_hysteresis(
        &self,
        upper_hysteresis: (bool, u32),
    ) -> zbus::Result<()> {
        let mut info = self
            .get_fan_curve_info()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        // Update hysteresis
        info.upper_threshold = dbus_opt(upper_hysteresis);

        self.set_fan_curve_info(info)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        Ok(())
    }
    #[zbus(property)]
    async fn set_lower_hysteresis(
        &self,
        lower_hysteresis: (bool, u32),
    ) -> zbus::Result<()> {
        let mut info = self
            .get_fan_curve_info()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        // Update hysteresis
        info.lower_threshold = dbus_opt(lower_hysteresis);

        self.set_fan_curve_info(info)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        Ok(())
    }
}

impl FanCurveInterface {
    pub fn new(
        curve_name: String,
        tx_config_manager: Sender<ConfigMessage>,
    ) -> Self {
        Self {
            curve_name,
            tx_config_manager,
        }
    }

    // Return the fan curve info
    async fn get_fan_curve_info(&self) -> Result<FanCurveInfo> {
        let (tx, rx) = oneshot::channel();
        let message = ConfigMessage::GetFanCurve {
            fan_curve: self.curve_name.clone(),
            tx,
        };

        self.tx_config_manager.send(message).await?;
        let fan_curve_answer = rx.await?;

        let fan_curve_opt =
            extract_answer!(ConfigMessageAnswer::FanCurve, fan_curve_answer)?;

        fan_curve_opt.ok_or_else(|| {
            anyhow!("The {} curve doesn't exist!", self.curve_name)
        })
    }

    // Set the given fan curve info
    async fn set_fan_curve_info(&self, curve_info: FanCurveInfo) -> Result<()> {
        let message = ConfigMessage::SetFanCurve {
            curve_name: self.curve_name.clone(),
            curve: curve_info,
        };
        self.tx_config_manager.send(message).await?;

        Ok(())
    }
}
