use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
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

const DEFAULT: &str = "default";

const GPUS_JSON: &str = "gpus";
const FAN_CURVES_JSON: &str = "fan_curves";
const PROFILES_JSON: &str = "profiles";

// Store the answer to the configuration request
#[derive(Debug)]
pub enum ConfigMessageAnswer {
    ListGpus(Vec<String>),
    ListFanCurves(Vec<String>),
    ListProfiles(Vec<String>),

    Gpu(GpuConfig),
    FanCurve(FanCurveConfig),
    Profile(ProfileConfig),
}

type Responder = oneshot::Sender<ConfigMessageAnswer>;

// TODO: better documentation
#[derive(Debug)]
pub enum ConfigMessage {
    // Requires a list of the GPU UUIDs in the configuration
    ListGpus(Responder),
    // Requires the fan curves in the configuration
    ListFanCurves(Responder),
    // Requires the profiles  in the configuration
    ListProfiles(Responder),

    // These functions returns a default configuration if the 
    // requested UUID or name aren't specified in the configuration
    GetGpu { uuid: String, tx: Responder },
    GetFanCurve { name: String, tx: Responder },
    GetProfile { name: String, tx: Responder },

    // These functions automatically add the requested objects
    // to the corresponding lists if they don't already exist
    SetGpu(GpuConfig),
    SetFanCurve(FanCurveConfig),
    SetProfile(ProfileConfig),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GpuConfig {
    pub uuid: String,
    pub profile: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProfileConfig {
    pub name: String,

    pub fan_curve: String,

    pub power_limit: Option<u32>,
    pub core_offset: Option<i32>,
    pub mem_offset: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FanCurveConfig {
    pub name: String,

    pub manual: bool,
    pub points: Vec<(u32, u32)>,
    pub hysteresis_up: u32,
    pub hysteresis_down: u32,
}

// Manage the stored daemon Json configuration
pub struct ConfigManager {
    config_path: PathBuf,

    // Stored as UUID
    gpu_configs: HashMap<String, GpuConfig>,
    fan_curve_configs: HashMap<String, FanCurveConfig>,
    profile_configs: HashMap<String, ProfileConfig>,
}

impl ConfigManager {
    // Create a new configuration manager
    pub fn new(config_path: &Path) -> Self {
        Self {
            config_path: config_path.to_path_buf(),

            gpu_configs: HashMap::new(),
            fan_curve_configs: HashMap::new(),
            profile_configs: HashMap::new(),
        }
    }

    // Run the configuration manager
    pub async fn run(
        &mut self,
        run_token: CancellationToken,
        mut rx_message: Receiver<ConfigMessage>,
        tx_err: Sender<anyhow::Error>,
    ) {
        info!("Config manager: Running");

        // Parse the config file specified at creation time
        if let Err(err) = self.parse_config_file() {
            tx_err.send(err).await.unwrap_or_else(|err| {
                error!("Failed to send error over channel: {err}");
            });
        }

        trace!("Current profile configs: {:?}", self.profile_configs);
        trace!("Current fan curve configs: {:?}", self.fan_curve_configs);
        trace!("Current gpu configs: {:?}", self.gpu_configs);

        self.save_config().unwrap();

        loop {
            select! {
                _ = run_token.cancelled() => {
                    info!("Config manager: Quiting");

                    break;
                },
                message = rx_message.recv() => {
                    if let Err(err) = self.parse_message(message) {
                        tx_err.send(err).await.unwrap_or_else(|err| {
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
                ConfigMessage::ListGpus(_) => {
                    self.handle_list_message(message)?;
                }
                ConfigMessage::ListProfiles(_) => {
                    self.handle_list_message(message)?;
                }
                ConfigMessage::ListFanCurves(_) => {
                    self.handle_list_message(message)?;
                }

                ConfigMessage::GetGpu { uuid: _, tx: _ } => {
                    self.handle_get_message(message)?;
                }
                ConfigMessage::GetProfile { name: _, tx: _ } => {
                    self.handle_get_message(message)?;
                }
                ConfigMessage::GetFanCurve { name: _, tx: _ } => {
                    self.handle_get_message(message)?;
                }

                ConfigMessage::SetGpu(_) => {
                    self.hadle_set_message(message)?;
                }
                ConfigMessage::SetProfile(_) => {
                    self.hadle_set_message(message)?;
                }
                ConfigMessage::SetFanCurve(_) => {
                    self.hadle_set_message(message)?;
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
            ConfigMessage::SetGpu(config) => {
                let uuid = config.uuid.clone();
                self.gpu_configs.insert(uuid, config);
            }
            ConfigMessage::SetProfile(config) => {
                let name = config.name.clone();
                self.profile_configs.insert(name, config);
            }
            ConfigMessage::SetFanCurve(config) => {
                let name = config.name.clone();
                self.fan_curve_configs.insert(name, config);
            }
            _ => {
                return Err(anyhow!(
                    "Called handle_set_message on wrong message type"
                ));
            }
        }

        // Save the current configuration state to the config file
        self.save_config()?;

        Ok(())
    }

    // Return a default configuration if the config is not in the hash map
    fn handle_get_message(&self, message: ConfigMessage) -> Result<()> {
        let (tx, answer) = match message {
            ConfigMessage::GetGpu { uuid, tx } => {
                let gpu =
                    self.gpu_configs.get(uuid.as_str()).cloned().unwrap_or(
                        GpuConfig {
                            uuid: uuid,
                            profile: DEFAULT.to_string(),
                        },
                    );

                (tx, ConfigMessageAnswer::Gpu(gpu))
            }
            ConfigMessage::GetFanCurve { name, tx } => {
                let fan_curve = self
                    .fan_curve_configs
                    .get(name.as_str())
                    .cloned()
                    .unwrap_or(FanCurveConfig::default());

                (tx, ConfigMessageAnswer::FanCurve(fan_curve))
            }
            ConfigMessage::GetProfile { name, tx } => {
                let profile = self
                    .profile_configs
                    .get(name.as_str())
                    .cloned()
                    .unwrap_or(ProfileConfig::default());

                (tx, ConfigMessageAnswer::Profile(profile))
            }
            _ => {
                return Err(anyhow!(
                    "Called handle_get_message on wrong message type"
                ));
            }
        };

        // Send data to the channel
        tx.send(answer).map_err(|v| {
            anyhow!("Failed to send answer to channel: {v:?}")
        })?;

        Ok(())
    }

    fn handle_list_message(&self, message: ConfigMessage) -> Result<()> {
        let mut answer_list = Vec::new();

        let (tx, answer) = match message {
            ConfigMessage::ListGpus(tx) => {
                for (uuid, _) in self.gpu_configs.iter() {
                    answer_list.push(uuid.to_string());
                }

                (tx, ConfigMessageAnswer::ListGpus(answer_list))
            }
            ConfigMessage::ListFanCurves(tx) => {
                for (name, _) in self.fan_curve_configs.iter() {
                    answer_list.push(name.to_string());
                }

                (tx, ConfigMessageAnswer::ListFanCurves(answer_list))
            }
            ConfigMessage::ListProfiles(tx) => {
                for (name, _) in self.profile_configs.iter() {
                    answer_list.push(name.to_string());
                }

                (tx, ConfigMessageAnswer::ListProfiles(answer_list))
            }
            _ => {
                return Err(anyhow!(
                    "Called handle_list_message on wrong message type"
                ));
            }
        };

        // Send data to the channel
        tx.send(answer).map_err(|err| {
            anyhow!("Failed to send answer to channel: {err:?}")
        })?;

        Ok(())
    }

    fn parse_config_file(&mut self) -> Result<()> {
        debug!("Parsing config file at: {:?}", self.config_path);

        // Insert the default profile and fan curve in the tables
        self.profile_configs
            .insert(DEFAULT.to_string(), ProfileConfig::default());
        self.fan_curve_configs
            .insert(DEFAULT.to_string(), FanCurveConfig::default());

        // Read the file to a string
        let file = File::open(&self.config_path)
            .with_context(|| "Failed to open Json configuration file")?;

        let buf = BufReader::new(file);

        // Parse the Json data
        let config_json: Value = serde_json::from_reader(buf)
            .with_context(|| "Failed to parse Json configuration file")?;

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

        // Parse all of the GPU configurations entries
        if let Value::Array(gpus) = config_json[GPUS_JSON].clone() {
            for gpu in gpus {
                if let Err(err) = self.parse_gpu(gpu) {
                    warn!("Failed to parse GPU config: {err}");
                }
            }
        }

        Ok(())
    }

    // Save the current configuration to the config file
    fn save_config(&self) -> Result<()> {
        let mut profiles = Vec::new();
        let mut fan_curves = Vec::new();
        let mut gpus = Vec::new();

        // Add profiles to the output
        for (name, profile) in self.profile_configs.iter() {
            // Ignore the profile if it's the default one
            if name == DEFAULT {
                continue;
            }

            profiles.push(profile);
        }

        // Add fan curves to the output
        for (name, fan_curve) in self.fan_curve_configs.iter() {
            // Ignore the fan curve if it's the default one
            if name == DEFAULT {
                continue;
            }

            fan_curves.push(fan_curve);
        }

        // Add GPUs to the output
        for (_, gpu) in self.gpu_configs.iter() {
            gpus.push(gpu);
        }

        // Create the Json object
        let config_json = json!({
            FAN_CURVES_JSON: fan_curves,
            PROFILES_JSON: profiles,
            GPUS_JSON: gpus,
        });

        // Save the Json object in the configuration file
        let file = File::create(&self.config_path)
            .with_context(|| "Failed to open configuration file for writing")?;
        serde_json::to_writer_pretty(file, &config_json)
            .with_context(|| "Failed to write to the configuration file")?;

        Ok(())
    }

    // Parse data relative to one profiles and add it to the
    // configuration manager hash map
    fn parse_profile(&mut self, profile_json: Value) -> Result<()> {
        let profile: ProfileConfig = serde_json::from_value(profile_json)?;

        // If the profile is already in the config ignore it
        if self.profile_configs.contains_key(profile.name.as_str()) {
            warn!(
                "Redefinition of profile: \"{}\", ignoring it",
                profile.name.as_str()
            );

            return Ok(());
        }

        self.profile_configs.insert(profile.name.clone(), profile);

        Ok(())
    }

    // Parse data relative to one fan curve and add it to the
    // configuration manager hash map
    fn parse_fan_curve(&mut self, fan_curve_json: Value) -> Result<()> {
        let fan_curve: FanCurveConfig = serde_json::from_value(fan_curve_json)?;

        // If the fan curve is already in the config ignore it
        if self.fan_curve_configs.contains_key(fan_curve.name.as_str()) {
            warn!(
                "Redefinition of fan curve: \"{}\", ignoring it",
                fan_curve.name.as_str()
            );

            return Ok(());
        }

        self.fan_curve_configs
            .insert(fan_curve.name.clone(), fan_curve);

        Ok(())
    }

    // Parse data relative to one GPU configuration and add it to the
    // configuration manager hash map
    fn parse_gpu(&mut self, gpu_json: Value) -> Result<()> {
        let gpu: GpuConfig = serde_json::from_value(gpu_json)?;

        // If the GPU is already in the config ignore it
        if self.gpu_configs.contains_key(gpu.uuid.as_str()) {
            warn!(
                "Redefinition of GPU: \"{}\", ignoring it",
                gpu.uuid.as_str()
            );

            return Ok(());
        }

        self.gpu_configs.insert(gpu.uuid.clone(), gpu);

        Ok(())
    }
}

impl Default for FanCurveConfig {
    fn default() -> Self {
        Self {
            name: DEFAULT.to_string(),

            manual: false,
            points: vec![],
            hysteresis_up: 0,
            hysteresis_down: 0,
        }
    }
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            name: DEFAULT.to_string(),

            fan_curve: DEFAULT.to_string(),
            power_limit: None,
            core_offset: None,
            mem_offset: None,
        }
    }
}
