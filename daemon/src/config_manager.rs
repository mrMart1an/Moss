use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt::Debug,
    path::{Path, PathBuf},
    time::Duration,
};

use tokio::{
    select,
    sync::{
        mpsc::Receiver,
        oneshot,
    },
};
use tokio_util::sync::CancellationToken;

use tracing::{debug, error, info, trace};

use crate::{
    fan_curve::{fan_curve_info::FanCurveInfo, fan_mode::FanMode},
    gpu_device::{
        DEFAULT_DATA_UPDATE_INTERVAL, DEFAULT_FAN_UPDATE_INTERVAL,
        gpu_config::GpuConfig,
    },
};

// Alias the result type for this module
type Result<T> = std::result::Result<T, anyhow::Error>;

// Store the answer to the configuration request
#[derive(Debug)]
pub enum ConfigMessageAnswer {
    FanMode(FanMode),
    FanCurve(Option<FanCurveInfo>),
    FanUpdateInterval(Option<Duration>),
    DataUpdateInterval(Option<Duration>),
    DeviceConfig(Option<GpuConfig>),
}

type Responder = oneshot::Sender<ConfigMessageAnswer>;

// TODO: better documentation
#[derive(Debug)]
pub enum ConfigMessage {
    // Get the fan mode for the given device
    // Return None if the device doesn't exist in the configuration
    GetFanMode {
        uuid: String,
        tx: Responder,
    },
    // Get the fan curve for the given device
    // Return None if the device doesn't exist in the configuration
    GetFanCurve {
        uuid: String,
        tx: Responder,
    },
    // Get the config for the given device
    // Return None if the device doesn't exist in the configuration
    GetDeviceConfig {
        uuid: String,
        tx: Responder,
    },
    // Get the fan update interval for the given device
    // Return None if the device doesn't exist in the configuration
    GetFanUpdateInterval {
        uuid: String,
        tx: Responder,
    },
    // Get the data update interval for the given device
    // Return None if the device doesn't exist in the configuration
    GetDataUpdateInterval {
        uuid: String,
        tx: Responder,
    },

    // Assign the given profile on the given device
    AssignProfile {
        uuid: String,
        profile: String,
    },
    // Set a fan mode for a profile
    SetProfileFanMode {
        profile: String,
        mode: FanMode,
    },
    // Set a fan curve for a profile
    SetProfileFanCurve {
        profile: String,
        curve_name: Option<String>,
    },
    SetFanUpdateInterval {
        profile: String,
        update_intrerval: Duration,
    },
    SetDataUpdateInterval {
        profile: String,
        update_intrerval: Duration,
    },
    // Set a config for a profile
    SetProfileDeviceConfig {
        profile: String,
        config: Option<GpuConfig>,
    },
    // Update or add a new fan curve with the given name
    SetFanCurve {
        curve_name: String,
        curve: FanCurveInfo,
    },

    // Save the configuration changes on the file
    SaveConfig,
}

// Internal parsed data types

// The GPU data type is also used for serialization
#[derive(Debug, Serialize, Deserialize, Clone)]
struct DeviceConfig {
    pub uuid: String,
    pub profile: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ProfileConfig {
    pub fan_mode: FanMode,
    pub fan_curve: Option<String>,

    pub device_config: Option<GpuConfig>,

    pub fan_update_interval: Duration,
    pub data_update_interval: Duration,
}

// The daemon config struct managed by confy
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct DaemonConfig {
    // Stored as UUID
    device_profiles: HashMap<String, DeviceConfig>,
    // Stored as names
    profile_configs: HashMap<String, ProfileConfig>,
    fan_curve_configs: HashMap<String, FanCurveInfo>,
}

// Manage the stored daemon Json configuration
pub struct ConfigManager {
    config_path: PathBuf,

    config: DaemonConfig,
}

impl ConfigManager {
    // Create a new configuration manager
    pub fn new(config_path: &Path) -> Self {
        Self {
            config_path: config_path.to_path_buf(),
            config: DaemonConfig::default(),
        }
    }

    // Run the configuration manager
    pub async fn run(
        &mut self,
        run_token: CancellationToken,
        mut rx_message: Receiver<ConfigMessage>,
    ) {
        info!("Config manager: Running");

        // Parse the config file specified at creation time
        self.parse_config_file().unwrap_or_else(|e| {
            error!("Initialization config parsing failed! {}", e)
        });

        trace!("Current device profiles: {:?}", self.config.device_profiles);
        trace!("Current profile configs: {:?}", self.config.profile_configs);
        trace!(
            "Current fan curve configs: {:?}",
            self.config.fan_curve_configs
        );

        loop {
            select! {
                _ = run_token.cancelled() => {
                    info!("Config manager: Quitting");

                    break;
                },
                message = rx_message.recv() => {
                    if let Some(message) = message {
                        self.parse_message(message).unwrap_or_else(|e| {
                            error!("Failed to send error over channel: {}", e);
                        });
                    }
                }
            }
        }
    }

    // Parse a message by dispatching it to the appropriate handler
    fn parse_message(&mut self, message: ConfigMessage) -> Result<()> {
        match message {
            ConfigMessage::SetProfileFanMode { profile, mode } => {
                let profile_config =
                    self.config.profile_configs.get_mut(&profile);

                if let Some(profile_config) = profile_config {
                    profile_config.fan_mode = mode;
                } else {
                    // Create e new profile if it doesn't already exist
                    let mut new_profile = ProfileConfig::default();
                    new_profile.fan_mode = mode;

                    self.config.profile_configs.insert(profile, new_profile);
                }

                return Ok(());
            }
            ConfigMessage::SetProfileFanCurve {
                profile,
                curve_name,
            } => {
                let profile_config =
                    self.config.profile_configs.get_mut(&profile);

                if let Some(profile_config) = profile_config {
                    profile_config.fan_curve = curve_name;
                } else {
                    // Create e new profile if it doesn't already exist
                    let mut new_profile = ProfileConfig::default();
                    new_profile.fan_curve = curve_name;

                    self.config.profile_configs.insert(profile, new_profile);
                }

                return Ok(());
            }
            ConfigMessage::SetFanUpdateInterval {
                profile,
                update_intrerval,
            } => {
                let profile_config =
                    self.config.profile_configs.get_mut(&profile);

                if let Some(profile_config) = profile_config {
                    profile_config.fan_update_interval = update_intrerval;
                } else {
                    // Create e new profile if it doesn't already exist
                    let mut new_profile = ProfileConfig::default();
                    new_profile.fan_update_interval = update_intrerval;

                    self.config.profile_configs.insert(profile, new_profile);
                }

                return Ok(());
            }
            ConfigMessage::SetDataUpdateInterval {
                profile,
                update_intrerval,
            } => {
                let profile_config =
                    self.config.profile_configs.get_mut(&profile);

                if let Some(profile_config) = profile_config {
                    profile_config.data_update_interval = update_intrerval;
                } else {
                    // Create e new profile if it doesn't already exist
                    let mut new_profile = ProfileConfig::default();
                    new_profile.data_update_interval = update_intrerval;

                    self.config.profile_configs.insert(profile, new_profile);
                }

                return Ok(());
            }
            ConfigMessage::SetProfileDeviceConfig { profile, config } => {
                let profile_config =
                    self.config.profile_configs.get_mut(&profile);

                if let Some(profile_config) = profile_config {
                    profile_config.device_config = config;
                } else {
                    // Create e new profile if it doesn't already exist
                    let mut new_profile = ProfileConfig::default();
                    new_profile.device_config = config;

                    self.config.profile_configs.insert(profile, new_profile);
                }

                return Ok(());
            }
            ConfigMessage::SetFanCurve { curve_name, curve } => {
                if let Some(curve_info) =
                    self.config.fan_curve_configs.get_mut(&curve_name)
                {
                    *curve_info = curve;
                } else {
                    self.config.fan_curve_configs.insert(curve_name, curve);
                };

                return Ok(());
            }

            _ => {}
        }

        // Parse Get message
        let (tx, answer) = match message {
            ConfigMessage::GetFanCurve { uuid, tx } => {
                let profile = self.get_profile(&uuid)?;

                let fan_curve_info = if let Some(name) = &profile.fan_curve {
                    self.config.fan_curve_configs.get(name).cloned()
                } else {
                    None
                };

                (tx, ConfigMessageAnswer::FanCurve(fan_curve_info))
            }
            ConfigMessage::GetFanMode { uuid, tx } => {
                let profile = self.get_profile(&uuid)?;

                let fan_mode = profile.fan_mode;

                (tx, ConfigMessageAnswer::FanMode(fan_mode))
            }
            ConfigMessage::GetFanUpdateInterval { uuid, tx } => {
                let profile = self.get_profile(&uuid)?;

                let updata_interval = Some(profile.fan_update_interval);

                (tx, ConfigMessageAnswer::FanUpdateInterval(updata_interval))
            }
            ConfigMessage::GetDataUpdateInterval { uuid, tx } => {
                let profile = self.get_profile(&uuid)?;

                let updata_interval = Some(profile.data_update_interval);

                (tx, ConfigMessageAnswer::DataUpdateInterval(updata_interval))
            }
            ConfigMessage::GetDeviceConfig { uuid, tx } => {
                let profile = self.get_profile(&uuid)?;

                let device_config = profile.device_config;

                (tx, ConfigMessageAnswer::DeviceConfig(device_config))
            }

            _ => {
                return Err(anyhow!(format!(
                    "Trying to parse unknown message"
                )));
            }
        };

        tx.send(answer).map_err(|_| {
            anyhow!("Failed to send answer on one shot channel")
        })?;

        Ok(())
    }

    fn get_profile(&self, uuid: &str) -> Result<ProfileConfig> {
        let device_profile = self.config.device_profiles.get(uuid);

        let profile = if let Some(data) = device_profile {
            if let Some(profile) =
                self.config.profile_configs.get(&data.profile)
            {
                profile.clone()
            } else {
                ProfileConfig::default()
            }
        } else {
            ProfileConfig::default()
        };

        Ok(profile)
    }

    fn parse_config_file(&mut self) -> Result<()> {
        debug!("Parsing config file at: {:?}", self.config_path);

        self.config =
            confy::load_path(self.config_path.clone()).with_context(|| {
                format!(
                    "config parsing error!: File: \"{:?}\"",
                    self.config_path.to_str()
                )
            })?;

        Ok(())
    }

    // Save the current configuration to the config file
    fn save_config(&self) -> Result<()> {
        debug!("Saving config file at: {:?}", self.config_path);

        confy::store_path(self.config_path.clone(), self.config.clone())
            .with_context(|| {
                format!(
                    "Config save error!: File: File: \"{:?}\"",
                    self.config_path.to_str()
                )
            })?;

        Ok(())
    }
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            fan_curve: None,
            device_config: None,
            fan_mode: FanMode::Auto,
            fan_update_interval: DEFAULT_FAN_UPDATE_INTERVAL,
            data_update_interval: DEFAULT_DATA_UPDATE_INTERVAL,
        }
    }
}
