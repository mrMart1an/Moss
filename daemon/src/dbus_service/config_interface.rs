use tokio::sync::mpsc::Sender;
use zbus::{Connection, interface};

use crate::{
    config_manager::ConfigMessage, devices_manager::DevicesManagerMessage,
};

pub struct ConfigInterface {
    connection: Connection,

    tx_device_manager: Sender<DevicesManagerMessage>,
    tx_config_manager: Sender<ConfigMessage>,
}

#[interface(name = "com.github.Mossd1.Config")]
impl ConfigInterface {
    // Apply the current configuration to all devices
    async fn apply_config(&self) -> zbus::fdo::Result<()> {
        let message = DevicesManagerMessage::ApplyConfigToAllDevices;

        self.tx_device_manager
            .send(message)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))
    }
    // Save the config to the file, also apply it to all devices
    async fn save_config(&self) -> zbus::fdo::Result<()> {
        // Apply the configuration to all devices
        let message = DevicesManagerMessage::ApplyConfigToAllDevices;

        self.tx_device_manager
            .send(message)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        // Save the configuration
        let message = ConfigMessage::SaveConfig;

        self.tx_config_manager
            .send(message)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))
    }
    // Revert the config to the one stored on the config file
    async fn revert_config(&self) -> zbus::fdo::Result<()> {
        // Revert the config to the one stored on file
        let message = ConfigMessage::RevertConfig;

        self.tx_config_manager
            .send(message)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        // Apply the reverted changes to the device manager
        let message = DevicesManagerMessage::ApplyConfigToAllDevices;

        self.tx_device_manager
            .send(message)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))
    }

}

impl ConfigInterface {
    pub fn new(
        connection: Connection,
        tx_device_manager: Sender<DevicesManagerMessage>,
        tx_config_manager: Sender<ConfigMessage>,
    ) -> Self {
        Self {
            connection,
            tx_device_manager,
            tx_config_manager,
        }
    }
}
