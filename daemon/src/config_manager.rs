use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt::Debug, path::PathBuf, time::Duration};

use tokio::{
    select,
    sync::{mpsc::Receiver, oneshot},
};
use tokio_util::sync::CancellationToken;

use tracing::{debug, error, info, trace};

use crate::{
    config, fan_curve::{fan_curve_info::FanCurveInfo, fan_mode::FanMode}, gpu_device::gpu_config::GpuConfig
};

// Alias the result type for this module
type Result<T> = std::result::Result<T, anyhow::Error>;

// Store the answer to the configuration request
#[derive(Debug)]
pub enum ConfigMessageAnswer {
    FanMode(FanMode),
    FanCurve(Option<FanCurveInfo>),
    FanCurveName(Option<String>),
    FanUpdateInterval(Option<Duration>),
    DataUpdateInterval(Option<Duration>),
    DeviceConfig(GpuConfig),

    ProfilesList(Vec<String>),
    FanCurvesList(Vec<String>),
}

type Responder = oneshot::Sender<ConfigMessageAnswer>;

// TODO: better documentation
#[derive(Debug)]
pub enum ConfigMessage {
    // Get the fan mode for the given device
    GetDeviceFanMode {
        uuid: String,
        tx: Responder,
    },
    // Get the fan curve for the given device
    // Return None if the device doesn't exist in the configuration
    GetDeviceFanCurve {
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
    GetDeviceFanUpdateInterval {
        uuid: String,
        tx: Responder,
    },
    // Get the data update interval for the given device
    // Return None if the device doesn't exist in the configuration
    GetDeviceDataUpdateInterval {
        uuid: String,
        tx: Responder,
    },

    // Profile getter functions
    // Get the fan mode for the given profile
    GetProfileFanMode {
        profile: String,
        tx: Responder,
    },
    // Get the fan curve name for the given profile
    // Return None if the profile doesn't exist in the configuration
    // or if the option isn't set
    GetProfileFanCurveName {
        profile: String,
        tx: Responder,
    },
    // Get the device config for the given profile
    // Return None if the profile doesn't exist in the configuration
    // or if the option isn't set
    GetProfileConfig {
        profile: String,
        tx: Responder,
    },
    // Get the fan update interval for the given profile
    // Return None if the profile doesn't exist in the configuration
    // or if the option isn't set
    GetProfileFanUpdateInterval {
        profile: String,
        tx: Responder,
    },
    // Get the data update interval for the given profile
    // Return None if the profile doesn't exist in the configuration
    // or if the option isn't set
    GetProfileDataUpdateInterval {
        profile: String,
        tx: Responder,
    },
    // Get a fan curve info with the specified name
    // Return None if the requested fan curve doesn't exist
    GetFanCurve {
        fan_curve: String,
        tx: Responder,
    },

    // List all the profile in the configuration
    // return a list of profile names
    ListProfiles {
        tx: Responder,
    },
    // List all the fan curves in the configuration
    // return a list of fan curve names
    ListFanCurves {
        tx: Responder,
    },

    // Assign the given profile on the given device
    SetDeviceProfile {
        uuid: String,
        profile: Option<String>,
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
    SetProfileFanUpdateInterval {
        profile: String,
        update_interval: Option<Duration>,
    },
    SetProfileDataUpdateInterval {
        profile: String,
        update_interval: Option<Duration>,
    },
    // Set a config for a profile
    SetProfileDeviceConfig {
        profile: String,
        config: GpuConfig,
    },
    // Update or add a new fan curve with the given name
    SetFanCurve {
        curve_name: String,
        curve: FanCurveInfo,
    },

    // Create a new profile
    CreateProfile {
        profile_name: String,
    },
    // Create a new fan curve
    CreateFanCurve {
        curve_name: String,
    },

    // rename a profile
    RenameProfile {
        old_name: String,
        new_name: String,
    },
    // Rename a fan curve
    RenameFanCurve {
        old_name: String,
        new_name: String,
    },

    // Delete a fan curve from the configuration
    DeleteFanCurve {
        curve_name: String,
    },
    // Delete a fan curve from the configuration
    DeleteProfile {
        profile_name: String,
    },

    // Save the configuration changes on the file
    SaveConfig,
    // Revert the configuration to the one stored on the file
    RevertConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ProfileConfig {
    pub fan_mode: FanMode,
    pub fan_curve: Option<String>,

    pub device_config: GpuConfig,

    pub fan_update_interval: Option<Duration>,
    pub data_update_interval: Option<Duration>,
}

// The daemon config struct managed by confy
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct DaemonConfig {
    // Stored as UUID
    device_profiles: HashMap<String, Option<String>>,
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
    pub fn new(config_path: Option<PathBuf>) -> Self {
        let path = if let Some(path) = config_path {
            path
        } else {
            PathBuf::from(config::default_config_path())
        };

        Self {
            config_path: path,
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

        // NOTE: test generation of config

        //self.config.device_profiles.insert(
        //    "GPU-75f6d20c-3cea-093e-c165-0185a79f3e86".to_string(),
        //    Some("myProfile".to_string()),
        //);

        //self.config.fan_curve_configs.insert(
        //    "myCurve".to_string(),
        //    FanCurveInfo {
        //        points: Vec::from([(40, 40), (50, 50), (60, 80), (75, 100)]),
        //        lower_threshold: Some(3),
        //        upper_threshold: Some(2),
        //    },
        //);

        //self.config.profile_configs.insert(
        //    "myProfile".to_string(),
        //    ProfileConfig {
        //        fan_mode: FanMode::Curve,
        //        fan_curve: Some("myCurve".to_string()),
        //        device_config: GpuConfig {
        //            nvidia_config: NvidiaConfig {
        //                core_clock_offset: None,
        //                mem_clock_offset: None,
        //            },
        //            power_limit: None,
        //        },
        //        fan_update_interval: Some(Duration::from_secs(1)),
        //        data_update_interval: Some(Duration::from_secs(1)),
        //    },
        //);

        //self.save_config().unwrap_or_else(|e| {
        //    error!("{}", e);
        //});

        // NOTE: test generation of config

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
        trace!("Parsing message: {:?}", message);

        let answer_packet = match message {
            // Handle get message
            ConfigMessage::GetDeviceFanCurve { uuid, tx } => {
                let profile = self.get_profile(&uuid);

                let fan_curve_info = if let Some(name) = &profile.fan_curve {
                    self.config.fan_curve_configs.get(name).cloned()
                } else {
                    None
                };

                Some((tx, ConfigMessageAnswer::FanCurve(fan_curve_info)))
            }
            ConfigMessage::GetDeviceFanMode { uuid, tx } => {
                let profile = self.get_profile(&uuid);

                let fan_mode = profile.fan_mode;

                Some((tx, ConfigMessageAnswer::FanMode(fan_mode)))
            }
            ConfigMessage::GetDeviceFanUpdateInterval { uuid, tx } => {
                let profile = self.get_profile(&uuid);

                let updata_interval = profile.fan_update_interval;

                Some((
                    tx,
                    ConfigMessageAnswer::FanUpdateInterval(updata_interval),
                ))
            }
            ConfigMessage::GetDeviceDataUpdateInterval { uuid, tx } => {
                let profile = self.get_profile(&uuid);

                let updata_interval = profile.data_update_interval;

                Some((
                    tx,
                    ConfigMessageAnswer::DataUpdateInterval(updata_interval),
                ))
            }
            ConfigMessage::GetDeviceConfig { uuid, tx } => {
                let profile = self.get_profile(&uuid);

                let device_config = profile.device_config;

                Some((tx, ConfigMessageAnswer::DeviceConfig(device_config)))
            }

            // Profile getters
            ConfigMessage::GetProfileFanMode { profile, tx } => {
                let fan_mode = if let Some(profile) =
                    self.config.profile_configs.get(&profile)
                {
                    profile.fan_mode
                } else {
                    FanMode::Auto
                };

                Some((tx, ConfigMessageAnswer::FanMode(fan_mode)))
            }
            ConfigMessage::GetProfileFanCurveName { profile, tx } => {
                let fan_curve = if let Some(profile) =
                    self.config.profile_configs.get(&profile)
                {
                    profile.fan_curve.clone()
                } else {
                    None
                };

                Some((tx, ConfigMessageAnswer::FanCurveName(fan_curve)))
            }
            ConfigMessage::GetProfileConfig { profile, tx } => {
                let config = if let Some(profile) =
                    self.config.profile_configs.get(&profile)
                {
                    profile.device_config.clone()
                } else {
                    GpuConfig::default()
                };

                Some((tx, ConfigMessageAnswer::DeviceConfig(config)))
            }
            ConfigMessage::GetProfileFanUpdateInterval { profile, tx } => {
                let fan_interval = if let Some(profile) =
                    self.config.profile_configs.get(&profile)
                {
                    profile.fan_update_interval
                } else {
                    None
                };

                Some((tx, ConfigMessageAnswer::FanUpdateInterval(fan_interval)))
            }
            ConfigMessage::GetProfileDataUpdateInterval { profile, tx } => {
                let data_interval = if let Some(profile) =
                    self.config.profile_configs.get(&profile)
                {
                    profile.data_update_interval
                } else {
                    None
                };

                Some((
                    tx,
                    ConfigMessageAnswer::DataUpdateInterval(data_interval),
                ))
            }
            ConfigMessage::GetFanCurve { fan_curve, tx } => {
                let fan_curve =
                    self.config.fan_curve_configs.get(&fan_curve).cloned();

                Some((tx, ConfigMessageAnswer::FanCurve(fan_curve)))
            }

            // List messages
            ConfigMessage::ListProfiles { tx } => {
                let mut list = Vec::new();

                for (profile, _) in self.config.profile_configs.iter() {
                    list.push(profile.clone());
                }

                Some((tx, ConfigMessageAnswer::ProfilesList(list)))
            }
            ConfigMessage::ListFanCurves { tx } => {
                let mut list = Vec::new();

                for (curve, _) in self.config.fan_curve_configs.iter() {
                    list.push(curve.clone());
                }

                Some((tx, ConfigMessageAnswer::FanCurvesList(list)))
            }

            // Handle set messages
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

                None
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

                None
            }
            ConfigMessage::SetProfileFanUpdateInterval {
                profile,
                update_interval: update_intrerval,
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

                None
            }
            ConfigMessage::SetProfileDataUpdateInterval {
                profile,
                update_interval: update_intrerval,
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

                None
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

                None
            }
            ConfigMessage::SetFanCurve { curve_name, curve } => {
                if let Some(curve_info) =
                    self.config.fan_curve_configs.get_mut(&curve_name)
                {
                    *curve_info = curve;
                } else {
                    self.config.fan_curve_configs.insert(curve_name, curve);
                };

                None
            }
            ConfigMessage::SetDeviceProfile { uuid, profile } => {
                self.config.device_profiles.insert(uuid, profile);

                None
            }

            // Delete messages
            ConfigMessage::DeleteFanCurve { curve_name } => {
                self.config.fan_curve_configs.remove(&curve_name);

                // Update all the reference to this profile to None
                for (_, profile_ref) in self.config.profile_configs.iter_mut() {
                    if let Some(fan_curve_ref) = &mut profile_ref.fan_curve {
                        if *fan_curve_ref == curve_name {
                            profile_ref.fan_curve = None;
                        }
                    }
                }

                None
            }
            ConfigMessage::DeleteProfile { profile_name } => {
                self.config.profile_configs.remove(&profile_name);

                // Update all the reference to this profile to None
                for (_, profile_ref_opt) in
                    self.config.device_profiles.iter_mut()
                {
                    if let Some(profile_ref) = profile_ref_opt {
                        if *profile_ref == profile_name {
                            *profile_ref_opt = None;
                        }
                    }
                }

                None
            }

            // Create messages
            ConfigMessage::CreateProfile { profile_name } => {
                self.config
                    .profile_configs
                    .insert(profile_name, ProfileConfig::default());

                None
            }
            ConfigMessage::CreateFanCurve { curve_name } => {
                self.config
                    .fan_curve_configs
                    .insert(curve_name, FanCurveInfo::default());

                None
            }

            // Rename messages
            ConfigMessage::RenameProfile { old_name, new_name } => {
                // Move the profile
                let old_profile = self
                    .config
                    .profile_configs
                    .get(&old_name)
                    .cloned()
                    .unwrap_or_default();

                self.config
                    .profile_configs
                    .insert(new_name.clone(), old_profile);

                // Delete the old one
                self.config.profile_configs.remove(&old_name);

                // Update all the reference to this profile to the new name
                for (_, profile_ref_opt) in
                    self.config.device_profiles.iter_mut()
                {
                    if let Some(profile_ref) = profile_ref_opt {
                        if *profile_ref == old_name {
                            *profile_ref = new_name.clone();
                        }
                    }
                }

                None
            }
            ConfigMessage::RenameFanCurve { old_name, new_name } => {
                // Move the profile
                let old_curve = self
                    .config
                    .fan_curve_configs
                    .get(&old_name)
                    .cloned()
                    .unwrap_or_default();

                self.config
                    .fan_curve_configs
                    .insert(new_name.clone(), old_curve);

                // Delete the old one
                self.config.fan_curve_configs.remove(&old_name);

                // Update all the reference to this profile to the new name
                for (_, profile_ref) in self.config.profile_configs.iter_mut() {
                    if let Some(fan_curve_ref) = &mut profile_ref.fan_curve {
                        if *fan_curve_ref == old_name {
                            *fan_curve_ref = new_name.clone();
                        }
                    }
                }

                None
            }

            // Config save message
            ConfigMessage::SaveConfig => {
                self.save_config()?;

                None
            }
            ConfigMessage::RevertConfig => {
                self.parse_config_file()?;

                None
            }
        };

        // Send the answer on the oneshot channel if needed
        if let Some((tx, answer)) = answer_packet {
            tx.send(answer).map_err(|_| {
                anyhow!("Failed to send answer on one shot channel")
            })?;
        }

        Ok(())
    }

    fn get_profile(&self, uuid: &str) -> ProfileConfig {
        let device_profile = self.config.device_profiles.get(uuid);

        let profile = if let Some(data) = device_profile.cloned() {
            if let Some(profile) =
                self.config.profile_configs.get(&data.unwrap_or_default())
            {
                profile.clone()
            } else {
                ProfileConfig::default()
            }
        } else {
            ProfileConfig::default()
        };

        profile
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
            device_config: GpuConfig::default(),
            fan_mode: FanMode::Auto,
            fan_update_interval: None,
            data_update_interval: None,
        }
    }
}
