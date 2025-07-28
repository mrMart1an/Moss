use core::fmt;
use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context, Result, anyhow};
use nvml_wrapper::{
    Device, Nvml, enum_wrappers::device::TemperatureSensor,
    enums::device::FanControlPolicy,
};
use tokio::{
    select,
    sync::mpsc::{Receiver, Sender},
};
use tokio_util::sync::CancellationToken;

use tracing::{error, info, trace, trace_span};

use crate::{
    device::GpuDevice,
    fan_curve::{FanCurve, FanSpeed},
};

#[derive(Debug, PartialEq)]
pub enum FanMode {
    Auto,
    Manual,
}

pub enum FanMessage {
    SetMode {
        uuid: String,
        mode: FanMode,
    },
    UpdateCurve {
        uuid: String,
        new_curve: Box<dyn FanCurve + Send>,
    },

    UpdateInterval {
        new_duration: Duration,
    },
}

// Store the configuration for a specific GPU on the system
struct GpuFanConfig {
    // Store the number of fan on the GPU
    fan_count: u32,

    mode: FanMode,
    curve: Option<Box<dyn FanCurve + Send>>,
}

pub struct FanManager {
    // NVML context, this can safely be accessed across threads
    nvml: Arc<Nvml>,
    // Update interval in seconds
    update_interval: Duration,

    // Store the configuration information for each GPU in the system
    // The key is the UUID of the GPU
    gpu_configs: HashMap<String, GpuFanConfig>,

    // Store the GPU devices needed by the manager
    devices: HashMap<String, GpuDevice>,
}

impl FanManager {
    pub fn new(nvml: Arc<Nvml>) -> Self {
        Self {
            nvml,
            update_interval: Duration::from_secs_f32(2.),

            gpu_configs: HashMap::new(),

            devices: HashMap::new(),
        }
    }

    // Run the fan manager
    pub async fn run(
        &mut self,
        run_token: CancellationToken,
        mut rx_cmd: Receiver<FanMessage>,
        tx_err: Sender<anyhow::Error>,
    ) {
        info!("Fan manager: Running");

        loop {
            select! {
                _ = run_token.cancelled() => {
                    info!("Fan manager: Quiting");

                    if let Err(err) = self.clenup() {
                        tx_err.send(err).await.unwrap_or_else(|err| {
                            error!("Failed to send error over channel: {err}");
                        });
                    }

                    break;
                },
                message = rx_cmd.recv() => {
                    let _guard = trace_span!("message parsing");

                    trace!("Parsing message: {:?}", message);

                    if let Err(err) = self.parse_msg(message) {
                        tx_err.send(err).await.unwrap_or_else(|err| {
                            error!("Failed to send error over channel: {err}");
                        });
                    }
                },
                _ = tokio::time::sleep(self.update_interval) => {
                    let _guard = trace_span!("updating");

                    // If any error occur send if to the error channel
                    if let Err(err) = self.update() {
                        tx_err.send(err).await.unwrap_or_else(|err| {
                            error!("Fan manager: Couldn't send error to channel: {err}")
                        });
                    }
                }
            }
        }
    }

    // Parse the receive message and apply the needed changes
    fn parse_msg(&mut self, message: Option<FanMessage>) -> Result<()> {
        // Check if the message was None
        if let Some(message) = message {
            match message {
                FanMessage::SetMode { uuid, mode } => {
                    // Get the device and config from the hash map
                    // or initialize a new one
                    let gpu_device = Self::get_device(
                        &self.nvml,
                        &mut self.devices,
                        uuid.as_str(),
                    )?;
                    let config = Self::get_config(
                        &gpu_device,
                        &mut self.gpu_configs,
                        uuid.as_str(),
                    )?;

                    Self::set_mode(gpu_device, config, mode)?;
                },
                FanMessage::UpdateCurve { uuid, new_curve } => {
                    // Get the device and config from the hash map
                    // or initialize a new one
                    let gpu_device = Self::get_device(
                        &self.nvml,
                        &mut self.devices,
                        uuid.as_str(),
                    )?;
                    let config = Self::get_config(
                        &gpu_device,
                        &mut self.gpu_configs,
                        uuid.as_str(),
                    )?;

                    config.curve = Some(new_curve);
                },
                FanMessage::UpdateInterval { new_duration } => {
                    trace!(
                        "New FanManager update interval: {:?}",
                        new_duration
                    );

                    self.update_interval = new_duration;
                },
            }
        }

        Ok(())
    }

    // Update the fan speed of all the GPU as needed
    fn update(&mut self) -> Result<()> {
        // Iterate over all of the device
        // The GPU configuration is assumed to be present from
        // the message parsing stage.
        for (uuid, gpu_device) in self.devices.iter() {
            let config = self.gpu_configs.get(uuid).ok_or_else(|| {
                anyhow!("Failed to fetch GPU config for {}", uuid)
            })?;

            // If the fan mode is Auto there is nothing to do
            if config.mode == FanMode::Auto {
                trace!("Running update for GPU: \"{}\" - Mode: Auto", uuid);
                continue;
            }

            // Otherwise fetch the temperature and set the
            // fan speed according to the fan curve
            let curve = config.curve.as_ref().ok_or_else(|| {
                anyhow!(
                    "Mode of GPU: \"{}\" is Auto but no fan curve set",
                    uuid
                )
            })?;

            let temp = Self::get_temp(&gpu_device)?;

            let fan_speed = curve.get_speed(temp);

            trace!(
                "Running update for GPU: \"{}\" - temp: {}°C - speed: {}",
                uuid,
                temp,
                fan_speed.get()
            );

            Self::set_speed(&gpu_device, config, fan_speed)?;
        }

        Ok(())
    }

    // Reset all of the GPU control policy to Auto before quitting
    fn clenup(&mut self) -> Result<()> {
        // Iterate over all of the device
        // The GPU configuration is assumed to be present from
        // the message parsing stage.
        for (uuid, gpu_device) in self.devices.iter() {
            let config = self.gpu_configs.get_mut(uuid).ok_or_else(|| {
                anyhow!("Failed to fetch GPU config for {}", uuid)
            })?;

            // If the fan mode is Auto there is nothing to do
            if config.mode != FanMode::Auto {
                trace!("Setting Mode to Auto for GPU \"{}\"", uuid);

                Self::set_mode(gpu_device, config, FanMode::Auto)?;
            }
        }

        Ok(())
    }

    // Set the fan speed for the given device
    fn set_speed(
        gpu_device: &GpuDevice,
        config: &GpuFanConfig,
        speed: FanSpeed,
    ) -> Result<()> {
        let mut device = gpu_device.get()?;

        // Change the fan speed only if the control mode is manual
        // Otherwise return an error
        if config.mode == FanMode::Auto {
            return Err(anyhow!(
                "Attempted to change fan speed while GPU mode was Auto"
            ));
        }

        // Apply the fan speed to all fans
        config.for_each_fan(&mut device, |dev, i| {
            dev.set_fan_speed(i, speed.get()).with_context(|| {
                format!("Failed to set fan speed for fan: {i}")
            })?;

            Ok(())
        })?;

        Ok(())
    }

    // Set the fan control mode to automatic,
    // in case of failure remain in manual mode.
    // The curve need to be not None for Manual mode.
    // Calling this function without a curve set will
    // result in an error if the requested FanMode is Manual
    fn set_mode(
        gpu_device: &GpuDevice,
        config: &mut GpuFanConfig,
        mode: FanMode,
    ) -> Result<()> {
        let mut device = gpu_device.get()?;

        // If the mode is already set return early
        if mode == config.mode {
            return Ok(());
        }

        if mode == FanMode::Manual && config.curve.is_none() {
            return Err(anyhow!(
                "Attempting to set fan mode to Manual withoud a curve set"
            ));
        }

        // Otherwise set the policy
        let policy = match mode {
            FanMode::Auto => FanControlPolicy::TemperatureContinousSw,
            FanMode::Manual => FanControlPolicy::Manual,
        };

        config.for_each_fan(&mut device, |dev, i| {
            dev.set_fan_control_policy(i, policy).with_context(|| {
                format!("Failed to set fan control policy for fan: {i}")
            })?;

            Ok(())
        })?;

        // If everything was successful set the new mode in the config
        config.mode = mode;

        Ok(())
    }

    // Retrieve the given GPU temperature
    fn get_temp(gpu_device: &GpuDevice) -> Result<u32> {
        let device = gpu_device.get()?;

        let temp = device
            .temperature(TemperatureSensor::Gpu)
            .with_context(|| "Failed to retive GPU temperature")?;

        Ok(temp)
    }

    // Get the device for the given UUID from the hash map
    // Attempt to create a device if it doesn't already exist
    fn get_device<'a>(
        nvml: &Arc<Nvml>,
        devices: &'a mut HashMap<String, GpuDevice>,
        uuid: &str,
    ) -> Result<&'a GpuDevice> {
        // Throwing error here should be impossible but checking just in case
        if devices.contains_key(uuid) {
            let device = devices
                .get(uuid)
                .ok_or(anyhow!("UUID not found"))
                .with_context(|| "This should never happen")?;

            Ok(device)
        } else {
            // Insert the device in the hash map
            let device = GpuDevice::new(nvml, uuid);
            devices.insert(uuid.to_string(), device);

            let device = devices
                .get(uuid)
                .ok_or(anyhow!("UUID not found"))
                .with_context(|| "This should never happen")?;

            Ok(device)
        }
    }

    // Get the device configuration for the given UUID from the hash map
    // Attempt to create a one if it doesn't already exist
    fn get_config<'a>(
        gpu_device: &GpuDevice,
        configs: &'a mut HashMap<String, GpuFanConfig>,
        uuid: &str,
    ) -> Result<&'a mut GpuFanConfig> {
        // Throwing error here should be impossible but checking just in case
        if configs.contains_key(uuid) {
            let gpu_fan_config = configs
                .get_mut(uuid)
                .ok_or(anyhow!("UUID not found"))
                .with_context(|| "This should never happen")?;

            Ok(gpu_fan_config)
        } else {
            // Insert the device in the hash map
            let gpu_fan_config = GpuFanConfig::try_new(gpu_device)?;
            configs.insert(uuid.to_string(), gpu_fan_config);

            let gpu_fan_config = configs
                .get_mut(uuid)
                .ok_or(anyhow!("UUID not found"))
                .with_context(|| "This should never happen")?;

            Ok(gpu_fan_config)
        }
    }
}

impl GpuFanConfig {
    // Initialize a GPU configuration struct with the given
    // device fan count and fan control policy
    fn try_new(gpu_device: &GpuDevice) -> Result<Self> {
        let device = gpu_device.get()?;

        let fan_count = device.num_fans()?;

        // If no fan are present return an error
        if fan_count == 0 {
            return Err(anyhow!("The GPU has no fans"));
        }

        let mode = match device.fan_control_policy(0)? {
            FanControlPolicy::TemperatureContinousSw => FanMode::Auto,
            FanControlPolicy::Manual => FanMode::Manual,
        };

        Ok(Self {
            fan_count,
            mode,
            curve: None,
        })
    }

    // Utility to execute a function for all fan on a GPU
    fn for_each_fan(
        &self,
        device: &mut Device<'_>,
        mut f: impl FnMut(&mut Device<'_>, u32) -> Result<()>,
    ) -> Result<()> {
        for i in 0..self.fan_count {
            f(device, i)?;
        }

        Ok(())
    }
}

impl fmt::Debug for FanMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        #[allow(dead_code)]
        #[derive(Debug)]
        enum FanMessage<'a> {
            SetMode { uuid: &'a String, mode: &'a FanMode },
            UpdateCurve { uuid: &'a String },

            UpdateInterval { new_duration: &'a Duration },
        }

        let msg = match self {
            Self::UpdateInterval { new_duration } => {
                FanMessage::UpdateInterval { new_duration }
            }
            Self::SetMode { uuid, mode } => FanMessage::SetMode { uuid, mode },
            Self::UpdateCurve { uuid, new_curve: _ } => {
                FanMessage::UpdateCurve { uuid }
            }
        };

        fmt::Debug::fmt(&msg, f)
    }
}
