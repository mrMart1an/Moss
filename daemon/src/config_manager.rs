use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt::Debug,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    time::Duration,
};
use thiserror::Error;

use serde_json::{Value, json};
use tokio::{
    select,
    sync::{
        mpsc::{Receiver, Sender},
        oneshot,
    },
};
use tokio_util::sync::CancellationToken;

use tracing::{debug, error, info, trace, warn};

use crate::{
    errors::MossdError,
    fan_curve::{fan_curve_info::FanCurveInfo, fan_mode::FanMode},
    gpu_device::{
        DEFAULT_FAN_UPDATE_INTERVAL,
        gpu_config::{GpuConfig, NvidiaConfig},
    },
};

const DEFAULT_PROFILE_NAME: &str = "default";

const GPUS_JSON: &str = "gpus";
const FAN_CURVES_JSON: &str = "fan_curves";
const PROFILES_JSON: &str = "profiles";
const CONFIGS_JSON: &str = "configs";

// Alias the result type for this module
type Result<T> = std::result::Result<T, ConfigError>;

// Configuration errors enum
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Configuration IO error: ({file}) {reason} - {error}")]
    IO {
        file: PathBuf,
        reason: String,
        error: anyhow::Error,
    },
    #[error("Configuration Json error: {reason} - {error}")]
    Json {
        reason: String,
        error: anyhow::Error,
    },
    #[error("Configuration set error: {reason}")]
    Set { reason: String },
    #[error("Configuration get error: {reason}")]
    Get { reason: String },
    #[error("Configuration TX error: {reason}")]
    TxError { reason: String },
}

// Store the answer to the configuration request
#[derive(Debug)]
pub enum ConfigMessageAnswer {
    FanMode(FanMode),
    FanCurve(Option<FanCurveInfo>),
    FanUpdateInterval(Option<Duration>),
    Config(Option<GpuConfig>),
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
    GetConfig {
        uuid: String,
        tx: Responder,
    },
    // Get the fan update interval for the given device
    // Return None if the device doesn't exist in the configuration
    GetFanUpdateInterval {
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
    // Set a config for a profile
    SetProfileConfig {
        profile: String,
        config_name: Option<String>,
    },
    // Update or add a new fan curve with the given name
    SetFanCurve {
        curve_name: String,
        curve: FanCurveInfo,
    },
    // Set a config for a profile
    SetConfig {
        config_name: String,
        config: GpuConfig,
    },

    // Save the configuration changes on the file
    SaveConfig,
}

// Internal parsed data types

// The GPU data type is also used for serialization
#[derive(Debug, Serialize, Deserialize, Clone)]
struct GpuData {
    pub uuid: String,
    pub profile: String,
}

#[derive(Debug)]
struct ProfileData {
    pub fan_mode: FanMode,
    pub fan_curve: Option<String>,
    pub config: Option<String>,
    pub update_interval: Duration,
}

// Json data types for serialization

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ProfileJson {
    pub name: String,

    pub fan_mode: FanModeJson,
    pub fan_curve: Option<String>,
    pub config: Option<String>,
    pub update_interval: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct FanModeJson {
    pub auto: Option<bool>,
    pub curve: Option<bool>,
    pub manual: Option<bool>,
    pub manaul_speed: Option<u8>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct FanCurveJson {
    pub name: String,

    pub points: Vec<(i32, u8)>,
    pub hysteresis_up: Option<u32>,
    pub hysteresis_down: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct NvidiaConfigJson {
    pub core_offset: Option<i32>,
    pub mem_offset: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ConfigJson {
    pub name: String,

    pub power_limit: Option<u32>,
    pub nvidia: Option<NvidiaConfigJson>,
}

// Manage the stored daemon Json configuration
pub struct ConfigManager {
    config_path: PathBuf,

    // Stored as UUID
    gpu_datas: HashMap<String, GpuData>,
    // Stored as names
    profile_datas: HashMap<String, ProfileData>,
    fan_curve_datas: HashMap<String, FanCurveInfo>,
    config_datas: HashMap<String, GpuConfig>,
}

impl ConfigManager {
    // Create a new configuration manager
    pub fn new(config_path: &Path) -> Self {
        Self {
            config_path: config_path.to_path_buf(),

            gpu_datas: HashMap::new(),
            fan_curve_datas: HashMap::new(),
            profile_datas: HashMap::new(),
            config_datas: HashMap::new(),
        }
    }

    // Run the configuration manager
    pub async fn run(
        &mut self,
        run_token: CancellationToken,
        mut rx_message: Receiver<ConfigMessage>,
        tx_err: Sender<MossdError>,
    ) {
        info!("Config manager: Running");

        // Parse the config file specified at creation time
        if let Err(err) = self.parse_config_file() {
            tx_err.send(err.into()).await.unwrap_or_else(|err| {
                error!("Failed to send error over channel: {err}");
            });
        }

        trace!("Current profile datas: {:?}", self.profile_datas);
        trace!("Current fan curve datas: {:?}", self.fan_curve_datas);
        trace!("Current gpu datas: {:?}", self.gpu_datas);
        trace!("Current config datas: {:?}", self.gpu_datas);

        loop {
            select! {
                _ = run_token.cancelled() => {
                    info!("Config manager: Quiting");

                    break;
                },
                message = rx_message.recv() => {
                    if let Err(err) = self.parse_message(message) {
                        tx_err.send(err.into()).await.unwrap_or_else(|err| {
                            error!("Failed to send error over channel: {err}");
                        });
                    }
                }
            }
        }
    }

    // Parse a message by dispatching it to the appropriate handler
    fn parse_message(&mut self, message: Option<ConfigMessage>) -> Result<()> {
        if let Some(message) = message {
            match message {
                ConfigMessage::GetFanMode { uuid: _, tx: _ } => {
                    self.handle_get_message(message)?;
                }
                ConfigMessage::GetFanCurve { uuid: _, tx: _ } => {
                    self.handle_get_message(message)?;
                }
                ConfigMessage::GetFanUpdateInterval { uuid: _, tx: _ } => {
                    self.handle_get_message(message)?;
                }
                ConfigMessage::GetConfig { uuid: _, tx: _ } => {
                    self.handle_get_message(message)?;
                }

                ConfigMessage::AssignProfile {
                    uuid: _,
                    profile: _,
                } => {
                    self.hadle_set_message(message)?;
                }
                ConfigMessage::SetProfileFanMode {
                    profile: _,
                    mode: _,
                } => {
                    self.hadle_set_message(message)?;
                }
                ConfigMessage::SetProfileFanCurve {
                    profile: _,
                    curve_name: _,
                } => {
                    self.hadle_set_message(message)?;
                }
                ConfigMessage::SetFanUpdateInterval {
                    profile: _,
                    update_intrerval: _,
                } => {
                    self.hadle_set_message(message)?;
                }
                ConfigMessage::SetProfileConfig {
                    profile: _,
                    config_name: _,
                } => {
                    self.hadle_set_message(message)?;
                }
                ConfigMessage::SetFanCurve {
                    curve_name: _,
                    curve: _,
                } => {
                    self.hadle_set_message(message)?;
                }
                ConfigMessage::SetConfig {
                    config_name: _,
                    config: _,
                } => {
                    self.hadle_set_message(message)?;
                }

                ConfigMessage::SaveConfig => {
                    self.save_config()?;
                }
            }
        } else {
            warn!("Attempting to parse empty message");
        }

        Ok(())
    }

    // Handle all incoming save message
    fn hadle_set_message(&mut self, message: ConfigMessage) -> Result<()> {
        match message {
            ConfigMessage::SetProfileFanMode { profile, mode } => {
                if profile == DEFAULT_PROFILE_NAME {
                    return Err(ConfigError::Set {
                        reason: format!("Can't modify default profile"),
                    });
                }

                let profile_data = self.profile_datas.get_mut(&profile);

                if let Some(profile_data) = profile_data {
                    profile_data.fan_mode = mode;
                } else {
                    // Create e new profile if it doesn't already exist
                    let mut new_profile = ProfileData::default();
                    new_profile.fan_mode = mode;

                    self.profile_datas.insert(profile, new_profile);
                }
            }
            ConfigMessage::SetProfileFanCurve {
                profile,
                curve_name,
            } => {
                if profile == DEFAULT_PROFILE_NAME {
                    return Err(ConfigError::Set {
                        reason: format!("Can't modify default profile"),
                    });
                }

                let profile_data = self.profile_datas.get_mut(&profile);

                if let Some(profile_data) = profile_data {
                    profile_data.fan_curve = curve_name;
                } else {
                    // Create e new profile if it doesn't already exist
                    let mut new_profile = ProfileData::default();
                    new_profile.fan_curve = curve_name;

                    self.profile_datas.insert(profile, new_profile);
                }
            }
            ConfigMessage::SetFanUpdateInterval {
                profile,
                update_intrerval,
            } => {
                if profile == DEFAULT_PROFILE_NAME {
                    return Err(ConfigError::Set {
                        reason: format!("Can't modify default profile"),
                    });
                }

                let profile_data = self.profile_datas.get_mut(&profile);

                if let Some(profile_data) = profile_data {
                    profile_data.update_interval = update_intrerval;
                } else {
                    // Create e new profile if it doesn't already exist
                    let mut new_profile = ProfileData::default();
                    new_profile.update_interval = update_intrerval;

                    self.profile_datas.insert(profile, new_profile);
                }
            }
            ConfigMessage::SetProfileConfig {
                profile,
                config_name,
            } => {
                if profile == DEFAULT_PROFILE_NAME {
                    return Err(ConfigError::Set {
                        reason: format!("Can't modify default profile"),
                    });
                }

                let profile_data = self.profile_datas.get_mut(&profile);

                if let Some(profile_data) = profile_data {
                    profile_data.config = config_name;
                } else {
                    // Create e new profile if it doesn't already exist
                    let mut new_profile = ProfileData::default();
                    new_profile.config = config_name;

                    self.profile_datas.insert(profile, new_profile);
                }
            }
            ConfigMessage::SetFanCurve { curve_name, curve } => {
                if let Some(curve_info) =
                    self.fan_curve_datas.get_mut(&curve_name)
                {
                    *curve_info = curve;
                } else {
                    self.fan_curve_datas.insert(curve_name, curve);
                };
            }
            ConfigMessage::SetConfig {
                config_name,
                config,
            } => {
                if let Some(config_data) =
                    self.config_datas.get_mut(&config_name)
                {
                    *config_data = config;
                } else {
                    self.config_datas.insert(config_name, config);
                };
            }
            _ => {
                return Err(ConfigError::Set {
                    reason: format!("Trying to parse unknow set message"),
                });
            }
        }

        Ok(())
    }

    // Return a default configuration if the config is not in the hash map
    fn handle_get_message(&self, message: ConfigMessage) -> Result<()> {
        let (tx, answer) = match message {
            ConfigMessage::GetFanCurve { uuid, tx } => {
                let profile = self.get_profile(&uuid)?;

                let fan_curve_info = if let Some(name) = &profile.fan_curve {
                    self.fan_curve_datas.get(name).cloned()
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

                let updata_interval = Some(profile.update_interval);

                (tx, ConfigMessageAnswer::FanUpdateInterval(updata_interval))
            }
            ConfigMessage::GetConfig { uuid, tx } => {
                let profile = self.get_profile(&uuid)?;

                let gpu_config = if let Some(name) = &profile.config {
                    self.config_datas.get(name).cloned()
                } else {
                    None
                };

                (tx, ConfigMessageAnswer::Config(gpu_config))
            }

            _ => {
                return Err(ConfigError::Get {
                    reason: format!("Trying to parse unknow get message"),
                });
            }
        };

        tx.send(answer).map_err(|_| ConfigError::TxError {
            reason: format!("Failed to send answer on oneshot channel"),
        })?;

        Ok(())
    }

    fn get_profile(&self, uuid: &str) -> Result<&ProfileData> {
        let gpu_data = self.gpu_datas.get(uuid);

        let profile = if let Some(data) = gpu_data {
            if let Some(profile) = self.profile_datas.get(&data.profile) {
                profile
            } else {
                self.profile_datas.get(DEFAULT_PROFILE_NAME).ok_or_else(
                    || ConfigError::Get {
                        reason: format!("Failed to fetch default profile"),
                    },
                )?
            }
        } else {
            self.profile_datas
                .get(DEFAULT_PROFILE_NAME)
                .ok_or_else(|| ConfigError::Get {
                    reason: format!("Failed to fetch default profile"),
                })?
        };

        Ok(profile)
    }

    fn parse_config_file(&mut self) -> Result<()> {
        debug!("Parsing config file at: {:?}", self.config_path);

        // Insert the default profile and fan curve in the tables
        self.profile_datas
            .insert(DEFAULT_PROFILE_NAME.to_string(), ProfileData::default());

        // Read the file to a string
        let file =
            File::open(&self.config_path).map_err(|e| ConfigError::IO {
                file: self.config_path.clone(),
                reason: format!("Failed to open configuration file"),
                error: e.into(),
            })?;

        let buf = BufReader::new(file);

        // Parse the Json data
        let config_json: Value =
            serde_json::from_reader(buf).map_err(|e| ConfigError::Json {
                reason: format!("Failed to load Json configuration file"),
                error: e.into(),
            })?;

        // Parse all of the GPU configurations entries
        if let Value::Array(gpus) = config_json[GPUS_JSON].clone() {
            for gpu in gpus {
                if let Err(err) = self.parse_gpu(gpu) {
                    warn!("Failed to parse GPU datas: {err}");
                }
            }
        }

        // Parse all of the profile configurations entries
        if let Value::Array(profiles) = config_json[PROFILES_JSON].clone() {
            for profile in profiles {
                if let Err(err) = self.parse_profile(profile) {
                    warn!("Failed to parse profile: {err}");
                }
            }
        }

        // Parse all of the fan curve configurations entries
        if let Value::Array(fan_curves) = config_json[FAN_CURVES_JSON].clone() {
            for fan_curve in fan_curves {
                if let Err(err) = self.parse_fan_curve(fan_curve) {
                    warn!("Failed to parse fan curve: {err}");
                }
            }
        }

        // Parse all of the config entries
        if let Value::Array(configs) = config_json[CONFIGS_JSON].clone() {
            for config in configs {
                if let Err(err) = self.parse_config(config) {
                    warn!("Failed to parse config: {err}");
                }
            }
        }

        Ok(())
    }

    // Save the current configuration to the config file
    fn save_config(&self) -> Result<()> {
        let mut gpus_json = Vec::new();
        let mut profiles_json = Vec::new();
        let mut fan_curves_json = Vec::new();
        let mut configs_json = Vec::new();

        // Add GPUs to the output
        for (_, gpu) in self.gpu_datas.iter() {
            gpus_json.push(gpu);
        }

        // Add profiles to the output
        for (name, profile) in self.profile_datas.iter() {
            // Ignore the profile if it's the default one
            if name == DEFAULT_PROFILE_NAME {
                continue;
            }

            let profile_json: ProfileJson = (name, profile).try_into()?;
            profiles_json.push(profile_json);
        }

        // Add fan curves to the output
        for (name, fan_curve) in self.fan_curve_datas.iter() {
            let fan_curve_json: FanCurveJson = (name, fan_curve).try_into()?;
            fan_curves_json.push(fan_curve_json);
        }

        // Add fan curves to the output
        for (name, config) in self.config_datas.iter() {
            let config_json: ConfigJson = (name, config).try_into()?;
            configs_json.push(config_json);
        }

        // Create the Json object
        let config_json = json!({
            GPUS_JSON: gpus_json,
            PROFILES_JSON: profiles_json,
            FAN_CURVES_JSON: fan_curves_json,
            CONFIGS_JSON: configs_json,
        });

        // Save the Json object in the configuration file
        let file =
            File::create(&self.config_path).map_err(|e| ConfigError::IO {
                file: self.config_path.clone(),
                reason: format!(
                    "Failed to open configuration file for writing"
                ),
                error: e.into(),
            })?;

        serde_json::to_writer_pretty(file, &config_json).map_err(|e| {
            ConfigError::Json {
                reason: format!("Failed to write to the configuration file"),
                error: e.into(),
            }
        })?;

        Ok(())
    }

    // Parse data relative to one profiles and add it to the
    // configuration manager hash map
    fn parse_profile(&mut self, profile_json: Value) -> Result<()> {
        let profile: ProfileJson = serde_json::from_value(profile_json)
            .map_err(|e| ConfigError::Json {
                reason: format!("Failed to parse profile Json"),
                error: e.into(),
            })?;

        // If the profile is already in the config ignore it
        if self.profile_datas.contains_key(profile.name.as_str()) {
            warn!(
                "Redefinition of profile: \"{}\", ignoring it",
                profile.name.as_str()
            );

            return Ok(());
        }

        self.profile_datas
            .insert(profile.name.clone(), profile.try_into()?);

        Ok(())
    }

    // Parse data relative to one fan curve and add it to the
    // configuration manager hash map
    fn parse_fan_curve(&mut self, fan_curve_json: Value) -> Result<()> {
        let fan_curve: FanCurveJson = serde_json::from_value(fan_curve_json)
            .map_err(|e| ConfigError::Json {
                reason: format!("Failed to parse fan curve Json"),
                error: e.into(),
            })?;

        // If the fan curve is already in the config ignore it
        if self.fan_curve_datas.contains_key(fan_curve.name.as_str()) {
            warn!(
                "Redefinition of fan curve: \"{}\", ignoring it",
                fan_curve.name.as_str()
            );

            return Ok(());
        }

        self.fan_curve_datas
            .insert(fan_curve.name.clone(), fan_curve.try_into()?);

        Ok(())
    }

    // Parse data relative to one GPU and add it to the
    // configuration manager hash map
    fn parse_gpu(&mut self, gpu_json: Value) -> Result<()> {
        let gpu: GpuData = serde_json::from_value(gpu_json).map_err(|e| {
            ConfigError::Json {
                reason: format!("Failed to parse GPU data Json"),
                error: e.into(),
            }
        })?;

        // If the GPU is already in the config ignore it
        if self.gpu_datas.contains_key(gpu.uuid.as_str()) {
            warn!(
                "Redefinition of GPU: \"{}\", ignoring it",
                gpu.uuid.as_str()
            );

            return Ok(());
        }

        self.gpu_datas.insert(gpu.uuid.clone(), gpu);

        Ok(())
    }

    // Parse data relative to one config and add it to the
    // configuration manager hash map
    fn parse_config(&mut self, config_json: Value) -> Result<()> {
        let config: ConfigJson =
            serde_json::from_value(config_json).map_err(|e| {
                ConfigError::Json {
                    reason: format!("Failed to parse config Json"),
                    error: e.into(),
                }
            })?;

        // If the config is already in the config ignore it
        if self.config_datas.contains_key(config.name.as_str()) {
            warn!(
                "Redefinition of config: \"{}\", ignoring it",
                config.name.as_str()
            );

            return Ok(());
        }

        self.config_datas
            .insert(config.name.clone(), config.try_into()?);

        Ok(())
    }
}

impl Default for ProfileData {
    fn default() -> Self {
        Self {
            fan_curve: None,
            config: None,
            fan_mode: FanMode::Auto,
            update_interval: DEFAULT_FAN_UPDATE_INTERVAL,
        }
    }
}

// Try from implementations - From json to data

impl TryFrom<FanModeJson> for FanMode {
    type Error = ConfigError;

    fn try_from(
        value: FanModeJson,
    ) -> std::result::Result<FanMode, Self::Error> {
        // Convert the fan mode
        let auto = value.auto.unwrap_or(false);
        let curve = value.curve.unwrap_or(false);
        let manual = value.manual.unwrap_or(false);

        trace!(
            "parsing fan mode: (auto: {}), (curve: {}), (manual: {})",
            auto, curve, manual
        );

        let fan_mode = if auto && !curve && !manual {
            FanMode::Auto
        } else if !auto && curve && !manual {
            FanMode::Curve
        } else if !auto && !curve && manual {
            let fan_speed = if let Some(speed) = value.manaul_speed {
                speed.clamp(0, 100)
            } else {
                return Err(ConfigError::Json {
                    reason: format!(
                        "Invalid fan mode: no fan speed for manual mode"
                    ),
                    error: anyhow!(
                        "Invalid fan mode: no fan speed for manual mode"
                    ),
                });
            };

            FanMode::Manual(fan_speed)
        } else {
            return Err(ConfigError::Json {
                reason: format!("Invalid fan mode"),
                error: anyhow!("Invalid fan mode"),
            });
        };

        Ok(fan_mode)
    }
}

impl TryFrom<NvidiaConfigJson> for NvidiaConfig {
    type Error = ConfigError;

    fn try_from(
        value: NvidiaConfigJson,
    ) -> std::result::Result<NvidiaConfig, Self::Error> {
        Ok(Self {
            core_clock_offset: value.core_offset,
            mem_clock_offset: value.mem_offset,
        })
    }
}

impl TryFrom<ProfileJson> for ProfileData {
    type Error = ConfigError;

    fn try_from(
        value: ProfileJson,
    ) -> std::result::Result<ProfileData, Self::Error> {
        let update_interval = if let Some(interval) = value.update_interval {
            Duration::from_secs_f32(interval)
        } else {
            DEFAULT_FAN_UPDATE_INTERVAL
        };

        Ok(Self {
            fan_mode: value.fan_mode.try_into()?,
            fan_curve: value.fan_curve,
            config: value.config,
            update_interval,
        })
    }
}

impl TryFrom<FanCurveJson> for FanCurveInfo {
    type Error = ConfigError;

    fn try_from(
        value: FanCurveJson,
    ) -> std::result::Result<FanCurveInfo, Self::Error> {
        Ok(Self {
            points: value.points,
            upper_threshold: value.hysteresis_up,
            lower_threshold: value.hysteresis_down,
        })
    }
}

impl TryFrom<ConfigJson> for GpuConfig {
    type Error = ConfigError;

    fn try_from(
        value: ConfigJson,
    ) -> std::result::Result<GpuConfig, Self::Error> {
        let nvidia_config = if let Some(nvidia) = value.nvidia {
            nvidia.try_into()?
        } else {
            NvidiaConfig::default()
        };

        Ok(Self {
            nvidia_config,
            power_limit: value.power_limit,
        })
    }
}

// Try from implementation - From data to json

impl TryFrom<NvidiaConfig> for NvidiaConfigJson {
    type Error = ConfigError;

    fn try_from(
        value: NvidiaConfig,
    ) -> std::result::Result<NvidiaConfigJson, Self::Error> {
        Ok(Self {
            core_offset: value.core_clock_offset,
            mem_offset: value.mem_clock_offset,
        })
    }
}

impl TryFrom<(&String, &GpuConfig)> for ConfigJson {
    type Error = ConfigError;

    fn try_from(
        value: (&String, &GpuConfig),
    ) -> std::result::Result<ConfigJson, Self::Error> {
        Ok(Self {
            name: value.0.clone(),
            power_limit: value.1.power_limit,
            nvidia: Some(value.1.nvidia_config.try_into()?),
        })
    }
}

impl TryFrom<FanMode> for FanModeJson {
    type Error = ConfigError;

    fn try_from(
        value: FanMode,
    ) -> std::result::Result<FanModeJson, Self::Error> {
        let mut fan_mode_json = FanModeJson::default();

        match value {
            FanMode::Auto => fan_mode_json.auto = Some(true),
            FanMode::Curve => fan_mode_json.curve = Some(true),
            FanMode::Manual(speed) => {
                fan_mode_json.manual = Some(true);
                fan_mode_json.manaul_speed = Some(speed)
            }
        }

        Ok(fan_mode_json)
    }
}

impl TryFrom<(&String, &ProfileData)> for ProfileJson {
    type Error = ConfigError;

    fn try_from(
        value: (&String, &ProfileData),
    ) -> std::result::Result<ProfileJson, Self::Error> {
        Ok(Self {
            name: value.0.clone(),
            fan_mode: value.1.fan_mode.try_into()?,
            fan_curve: value.1.fan_curve.clone(),
            config: value.1.config.clone(),
            update_interval: Some(value.1.update_interval.as_secs_f32()),
        })
    }
}

impl TryFrom<(&String, &FanCurveInfo)> for FanCurveJson {
    type Error = ConfigError;

    fn try_from(
        value: (&String, &FanCurveInfo),
    ) -> std::result::Result<FanCurveJson, Self::Error> {
        Ok(Self {
            name: value.0.clone(),
            points: value.1.points.clone(),
            hysteresis_up: value.1.upper_threshold,
            hysteresis_down: value.1.lower_threshold,
        })
    }
}
