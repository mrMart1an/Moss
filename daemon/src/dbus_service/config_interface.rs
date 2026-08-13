use tokio::sync::mpsc::Sender;
use zbus::{Connection, interface};

use crate::{
    config_manager::ConfigMessage,
    dbus_service::{
        CONFIG_OBJECT_PATH, FAN_CURVE_OBJECT_SUBPATH, PROFILE_OBJECT_SUBPATH,
        fan_curve_interface::FanCurveInterface,
        profile_interface::ProfileInterface,
        profile_nvidia_interface::ProfileNvidiaInterface,
    },
    devices_manager::DevicesManagerMessage,
};

pub struct ConfigInterface {
    connection: Connection,

    tx_device_manager: Sender<DevicesManagerMessage>,
    tx_config_manager: Sender<ConfigMessage>,

    profiles_list: Vec<String>,
    fan_curves_list: Vec<String>,
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

    // Profile management

    // Create a new profile with the given name,
    // NOTE: this function also revert all non saved change up to this point
    async fn create_profile(
        &mut self,
        profile_name: String,
    ) -> zbus::fdo::Result<()> {
        // Check if the name is already assigned
        if self.profiles_list.contains(&profile_name) {
            return Err(zbus::fdo::Error::Failed(format!(
                "The profile already exist!"
            )));
        }

        // Revert the configuration
        self.revert_config().await?;

        // Create the profile
        let message = ConfigMessage::CreateProfile {
            profile_name: profile_name.clone(),
        };

        self.tx_config_manager
            .send(message)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        // Save the new config
        self.save_config().await?;

        // Add the D-Bus profile object
        self.add_dbus_profile(profile_name).await?;

        Ok(())
    }

    // Delete a profile from the configuration
    // NOTE: this function also revert all non saved change up to this point
    async fn delete_profile(
        &mut self,
        profile_name: String,
    ) -> zbus::fdo::Result<()> {
        // Check if the name actually exist
        if !self.profiles_list.contains(&profile_name) {
            return Err(zbus::fdo::Error::Failed(format!(
                "The profile doesn't exist!"
            )));
        }

        // Revert the configuration
        self.revert_config().await?;

        // Create the profile
        let message = ConfigMessage::DeleteProfile {
            profile_name: profile_name.clone(),
        };

        self.tx_config_manager
            .send(message)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        // Save the new config
        self.save_config().await?;

        // Delete the D-Bus object
        self.delete_dbus_profile(profile_name).await?;

        Ok(())
    }

    // Rename a profile in the configuration
    // NOTE: this function also revert all non saved change up to this point
    async fn rename_profile(
        &mut self,
        old_name: String,
        new_name: String,
    ) -> zbus::fdo::Result<()> {
        // Check if the old name actually exist
        if !self.profiles_list.contains(&old_name) {
            return Err(zbus::fdo::Error::Failed(format!(
                "The profile doesn't exist!"
            )));
        }
        // Check if the new name doesn't already exist
        if self.profiles_list.contains(&new_name) {
            return Err(zbus::fdo::Error::Failed(format!(
                "The profile already exist!"
            )));
        }

        // Revert the configuration
        self.revert_config().await?;

        // Create the profile
        let message = ConfigMessage::RenameProfile {
            old_name: old_name.clone(),
            new_name: new_name.clone(),
        };

        self.tx_config_manager
            .send(message)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        // Save the new config
        self.save_config().await?;

        // Delete the old D-Bus object and create the new one
        self.delete_dbus_profile(old_name).await?;
        self.add_dbus_profile(new_name).await?;

        Ok(())
    }

    // Fan curve management

    // Create a new fan curve with the given name,
    // NOTE: this function also revert all non saved change up to this point
    async fn create_fan_curve(
        &mut self,
        curve_name: String,
    ) -> zbus::fdo::Result<()> {
        // Check if the name is already assigned
        if self.fan_curves_list.contains(&curve_name) {
            return Err(zbus::fdo::Error::Failed(format!(
                "The fan curve already exist!"
            )));
        }

        // Revert the configuration
        self.revert_config().await?;

        // Create the profile
        let message = ConfigMessage::CreateFanCurve {
            curve_name: curve_name.clone(),
        };

        self.tx_config_manager
            .send(message)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        // Save the new config
        self.save_config().await?;

        // Add the D-Bus profile object
        self.add_dbus_fan_curve(curve_name).await?;

        Ok(())
    }

    // Delete a profile from the configuration
    // NOTE: this function also revert all non saved change up to this point
    async fn delete_fan_curve(
        &mut self,
        curve_name: String,
    ) -> zbus::fdo::Result<()> {
        // Check if the name actually exist
        if !self.fan_curves_list.contains(&curve_name) {
            return Err(zbus::fdo::Error::Failed(format!(
                "The fan curve doesn't exist!"
            )));
        }

        // Revert the configuration
        self.revert_config().await?;

        // Create the profile
        let message = ConfigMessage::DeleteFanCurve {
            curve_name: curve_name.clone(),
        };

        self.tx_config_manager
            .send(message)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        // Save the new config
        self.save_config().await?;

        // Delete the D-Bus object
        self.delete_dbus_fan_curve(curve_name).await?;

        Ok(())
    }

    // Rename a profile in the configuration
    // NOTE: this function also revert all non saved change up to this point
    async fn rename_fan_curve(
        &mut self,
        old_name: String,
        new_name: String,
    ) -> zbus::fdo::Result<()> {
        // Check if the old name actually exist
        if !self.fan_curves_list.contains(&old_name) {
            return Err(zbus::fdo::Error::Failed(format!(
                "The fan curve doesn't exist!"
            )));
        }
        // Check if the new name doesn't already exist
        if self.fan_curves_list.contains(&new_name) {
            return Err(zbus::fdo::Error::Failed(format!(
                "The fan curve already exist!"
            )));
        }

        // Revert the configuration
        self.revert_config().await?;

        // Create the profile
        let message = ConfigMessage::RenameFanCurve {
            old_name: old_name.clone(),
            new_name: new_name.clone(),
        };

        self.tx_config_manager
            .send(message)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{}", e)))?;

        // Save the new config
        self.save_config().await?;

        // Delete the old D-Bus object and create the new one
        self.delete_dbus_fan_curve(old_name).await?;
        self.add_dbus_fan_curve(new_name).await?;

        Ok(())
    }
}

impl ConfigInterface {
    pub fn new(
        connection: Connection,
        tx_device_manager: Sender<DevicesManagerMessage>,
        tx_config_manager: Sender<ConfigMessage>,
        profiles_list: Vec<String>,
        fan_curves_list: Vec<String>,
    ) -> Self {
        Self {
            connection,

            tx_device_manager,
            tx_config_manager,

            profiles_list,
            fan_curves_list,
        }
    }

    async fn add_dbus_profile(
        &mut self,
        profile_name: String,
    ) -> zbus::fdo::Result<()> {
        // Generate new D-Bus object
        // Generate profile interface
        let profile_interface = ProfileInterface::new(
            profile_name.clone(),
            self.tx_config_manager.clone(),
        );
        if !self
            .connection
            .object_server()
            .at(
                format!(
                    "{}{}/{}",
                    CONFIG_OBJECT_PATH, PROFILE_OBJECT_SUBPATH, profile_name
                ),
                profile_interface,
            )
            .await?
        {
            return Err(zbus::fdo::Error::Failed(format!(
                "Failed to register ProfileInterace for {}",
                profile_name
            )));
        }

        // Generate profile Nvidia interface
        let profile_nvidia_interface = ProfileNvidiaInterface::new(
            profile_name.clone(),
            self.tx_config_manager.clone(),
        );
        if !self.connection
            .object_server()
            .at(
                format!(
                    "{}{}/{}",
                    CONFIG_OBJECT_PATH, PROFILE_OBJECT_SUBPATH, profile_name
                ),
                profile_nvidia_interface,
            )
            .await?
        {
            return Err(zbus::fdo::Error::Failed(format!(
                "Failed to register ProfileInterace for {}",
                profile_name
            )));
        }

        // Add the profile to the list
        self.profiles_list.push(profile_name);

        Ok(())
    }

    async fn delete_dbus_profile(
        &mut self,
        profile_name: String,
    ) -> zbus::fdo::Result<()> {
        // Delete the profile object from the D-Bus service
        self.connection
            .object_server()
            .remove::<ProfileInterface, _>(format!(
                "{}{}/{}",
                CONFIG_OBJECT_PATH, PROFILE_OBJECT_SUBPATH, profile_name
            ))
            .await?;

        self.connection
            .object_server()
            .remove::<ProfileNvidiaInterface, _>(format!(
                "{}{}/{}",
                CONFIG_OBJECT_PATH, PROFILE_OBJECT_SUBPATH, profile_name
            ))
            .await?;

        // Remove the profile from the profiles list
        if let Some(i) =
            self.profiles_list.iter().position(|x| *x == profile_name)
        {
            self.profiles_list.remove(i);
        }

        Ok(())
    }

    async fn add_dbus_fan_curve(
        &mut self,
        curve_name: String,
    ) -> zbus::fdo::Result<()> {
        // Generate new D-Bus object
        // Generate profile interface
        let curve_interface = FanCurveInterface::new(
            curve_name.clone(),
            self.tx_config_manager.clone(),
        );
        self.connection
            .object_server()
            .at(
                format!(
                    "{}{}/{}",
                    CONFIG_OBJECT_PATH, FAN_CURVE_OBJECT_SUBPATH, curve_name
                ),
                curve_interface,
            )
            .await?;

        // Add the profile to the list
        self.fan_curves_list.push(curve_name);

        Ok(())
    }

    async fn delete_dbus_fan_curve(
        &mut self,
        curve_name: String,
    ) -> zbus::fdo::Result<()> {
        // Delete the profile object from the D-Bus service
        self.connection
            .object_server()
            .remove::<FanCurveInterface, _>(format!(
                "{}{}/{}",
                CONFIG_OBJECT_PATH, FAN_CURVE_OBJECT_SUBPATH, curve_name
            ))
            .await?;

        // Remove the profile from the profiles list
        if let Some(i) =
            self.fan_curves_list.iter().position(|x| *x == curve_name)
        {
            self.fan_curves_list.remove(i);
        }

        Ok(())
    }
}
