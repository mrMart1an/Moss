use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use nvml_wrapper::Nvml;
use thiserror::Error;
use tokio::{
    select,
    sync::{
        mpsc::{Receiver, Sender},
        oneshot,
    },
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{
    errors::MossdError, fan_curve::{fan_mode::FanMode, FanCurve}, gpu_device::{
        gpu_config::GpuConfig, gpu_data::{GpuData, GpuVendorData}, gpu_info::{GpuInfo, GpuVendorInfo}, nvidia_device::NvidiaDevice, DeviceError, GpuDevice, DEFAULT_FAN_UPDATE_INTERVAL
    }
};

type Responder = oneshot::Sender<DevicesManagerAnswer>;

// Alias the result type for this module
type Result<T> = std::result::Result<T, DeviceManagerError>;

#[derive(Debug, Error)]
pub enum DeviceManagerError {
    #[error(transparent)]
    Device(#[from] DeviceError),
    #[error("Device manager discovery error: {reason}")]
    Discovery {
        reason: String,
        error: anyhow::Error,
    },
    #[error("Device manager channel TX error: {reason}")]
    TX { reason: String },
    #[error("Device manager channel invalid device error: {reason}")]
    InvalidDevice { reason: String },
}

pub enum DevicesManagerMessage {
    // List all the devices managed by the devices manager
    ListDevices {
        tx: Responder,
    },

    // Get the device general informations
    GetDeviceInfo {
        uuid: String,
        tx: Responder,
    },
    // Get the device vendor informations
    GetDeviceVendorInfo {
        uuid: String,
        tx: Responder,
    },

    // Get the device general data
    GetDeviceData {
        uuid: String,
        tx: Responder,
    },
    // Get the device vendor data
    GetDeviceVendorData {
        uuid: String,
        tx: Responder,
    },
    // Set the data update interval for the device
    SetDeviceDataUpdateInterval {
        uuid: String,
        interval: Duration,
    },

    // Set the device fan mode
    SetDeviceFanMode {
        uuid: String,
        fan_mode: FanMode,
    },
    // Set the device fan curve
    SetDeviceFanCurve {
        uuid: String,
        fan_curve: Box<dyn FanCurve + Send>,
    },
    // Set the fan update interval for the device
    SetDeviceFanUpdateInterval {
        uuid: String,
        interval: Duration,
    },

    // Apply the given GPU configuration to the device
    ApplyDeviceGpuConfig {
        uuid: String,
        config: GpuConfig,
    },
}

#[derive(Debug)]
pub enum DevicesManagerAnswer {
    DeviceList(Vec<String>),

    DeviceInfo(GpuInfo),
    DeviceVendorInfo(GpuVendorInfo),

    DeviceData(Option<GpuData>),
    DeviceVendorData(Option<GpuVendorData>),
}

pub struct DevicesManager {
    devices: HashMap<String, Box<dyn GpuDevice + Send>>,

    // Store the fan update interval for all the devices
    fan_update_intervals: HashMap<String, Duration>,
    // Store the last fan update instant for all the devices
    last_fan_updates: HashMap<String, Instant>,
}

impl DevicesManager {
    pub fn new() -> Self {
        // Attempt to initialize NVML
        let nvml = if let Ok(nvml) = Nvml::init() {
            info!("NVML successfully initialized");

            Some(Arc::new(nvml))
        } else {
            None
        };

        let mut devices: HashMap<String, Box<dyn GpuDevice + Send>> =
            HashMap::new();

        // If NVML was initialized find the Nvidia GPUs on the system
        if let Some(nvml) = nvml.clone() {
            Self::discover_nvidia_gpus(nvml, &mut devices).unwrap_or_else(
                |e| {
                    warn!("Error during Nvidia GPUs discovery: {}", e);
                },
            );
        }

        // Create a hash map with the default fan update interval
        // for each device
        // Create a hash map with the last fan update instant
        let mut fan_update_intervals = HashMap::new();
        let mut last_fan_updates = HashMap::new();

        for (uuid, device) in devices.iter_mut() {
            // Update the fan speed for the first time
            if let Err(e) = device.update_fan() {
                warn!(
                    "Error while updating fan speed on device creation: {}",
                    e
                )
            }

            last_fan_updates.insert(uuid.clone(), Instant::now());
            fan_update_intervals
                .insert(uuid.clone(), DEFAULT_FAN_UPDATE_INTERVAL);
        }

        Self {
            devices,
            fan_update_intervals,
            last_fan_updates,
        }
    }

    pub async fn run(
        &mut self,
        run_token: CancellationToken,
        mut rx_message: Receiver<DevicesManagerMessage>,
        tx_err: Sender<MossdError>,
    ) {
        let (mut next_fan_update_device, mut next_fan_update_time) =
            self.schedule_fan_update();

        loop {
            select! {
                _ = run_token.cancelled() => {
                    info!("devices manager: Quiting");

                    break;
                },
                message = rx_message.recv() => {
                    if let Err(err) = self.parse_message(message) {
                        tx_err.send(err.into()).await.unwrap_or_else(|err| {
                            error!("Failed to send error over channel: {err}");
                        });
                    }
                },
                // Update the fan and schedule the next update
                _ = tokio::time::sleep(next_fan_update_time) => {
                    if let Err(err) = self.update_fans(&next_fan_update_device) {
                        tx_err.send(err.into()).await.unwrap_or_else(|err| {
                            error!("Failed to send error over channel: {err}");
                        });
                    }

                    (next_fan_update_device, next_fan_update_time) =
                        self.schedule_fan_update();
                }
            }
        }
    }

    // Discover Nvidia GPUs on the system, create the associated
    // GPU devices and add them to the given hash map
    fn discover_nvidia_gpus(
        nvml: Arc<Nvml>,
        devices_map: &mut HashMap<String, Box<dyn GpuDevice + Send>>,
    ) -> Result<()> {
        let device_count = nvml.device_count().map_err(|e| {
            DeviceManagerError::Discovery {
                reason: format!("Failed to enumerate Nvidia devices"),
                error: e.into(),
            }
        })?;

        for i in 0..device_count {
            // Get the UUID of each device
            let device = nvml.device_by_index(i).map_err(|e| {
                DeviceManagerError::Discovery {
                    reason: format!("Failed to get Nvidia device"),
                    error: e.into(),
                }
            })?;
            let uuid = device.uuid().map_err(|e| {
                DeviceManagerError::Discovery {
                    reason: format!(
                        "Failed to get Nvidia device uuid (index: {})",
                        i
                    ),
                    error: e.into(),
                }
            })?;

            debug!("Found Nvidia device: \"{}\"", uuid);

            // Create the GPU device
            let device = Box::new(NvidiaDevice::new(nvml.clone(), &uuid)?);

            // Add the device to the hash map
            devices_map.insert(uuid, device);
        }

        Ok(())
    }

    // Parse and eventually answer to incoming messages
    fn parse_message(
        &mut self,
        message: Option<DevicesManagerMessage>,
    ) -> Result<()> {
        if message.is_none() {
            warn!("GPUs manager: parsing empty message");
            return Ok(());
        }

        match message.unwrap() {
            DevicesManagerMessage::ListDevices { tx } => {
                let mut devices_list = Vec::new();

                for (uuid, _) in self.devices.iter() {
                    devices_list.push(uuid.clone());
                }

                let answer = DevicesManagerAnswer::DeviceList(devices_list);
                tx.send(answer).map_err(|v| DeviceManagerError::TX {
                    reason: format!(
                        "Failed to send answer over channel: ({:?})",
                        v
                    ),
                })?
            }

            DevicesManagerMessage::GetDeviceInfo { uuid, tx } => {
                let device = self.devices.get(&uuid).ok_or_else(|| {
                    DeviceManagerError::InvalidDevice {
                        reason: format!("Trying to access non-existing device"),
                    }
                })?;

                let answer =
                    DevicesManagerAnswer::DeviceInfo(device.get_info());
                tx.send(answer).map_err(|v| DeviceManagerError::TX {
                    reason: format!(
                        "Failed to send answer over channel: ({:?})",
                        v
                    ),
                })?
            }
            DevicesManagerMessage::GetDeviceVendorInfo { uuid, tx } => {
                let device = self.devices.get(&uuid).ok_or_else(|| {
                    DeviceManagerError::InvalidDevice {
                        reason: format!("Trying to access non-existing device"),
                    }
                })?;

                let answer = DevicesManagerAnswer::DeviceVendorInfo(
                    device.get_vendor_info(),
                );
                tx.send(answer).map_err(|v| DeviceManagerError::TX {
                    reason: format!(
                        "Failed to send answer over channel: ({:?})",
                        v
                    ),
                })?
            }

            DevicesManagerMessage::GetDeviceData { uuid, tx } => {
                let device = self.devices.get_mut(&uuid).ok_or_else(|| {
                    DeviceManagerError::InvalidDevice {
                        reason: format!("Trying to access non-existing device"),
                    }
                })?;

                // TODO: Report error on failure
                let answer =
                    DevicesManagerAnswer::DeviceData(device.get_data().ok());
                tx.send(answer).map_err(|v| DeviceManagerError::TX {
                    reason: format!(
                        "Failed to send answer over channel: ({:?})",
                        v
                    ),
                })?
            }
            DevicesManagerMessage::GetDeviceVendorData { uuid, tx } => {
                let device = self.devices.get_mut(&uuid).ok_or_else(|| {
                    DeviceManagerError::InvalidDevice {
                        reason: format!("Trying to access non-existing device"),
                    }
                })?;

                // TODO: Report error on failure
                let answer = DevicesManagerAnswer::DeviceVendorData(
                    device.get_vendor_data().ok(),
                );
                tx.send(answer).map_err(|v| DeviceManagerError::TX {
                    reason: format!(
                        "Failed to send answer over channel: ({:?})",
                        v
                    ),
                })?
            }
            DevicesManagerMessage::SetDeviceDataUpdateInterval {
                uuid,
                interval,
            } => {
                let device = self.devices.get_mut(&uuid).ok_or_else(|| {
                    DeviceManagerError::InvalidDevice {
                        reason: format!("Trying to access non-existing device"),
                    }
                })?;

                device.set_data_update_interval(interval);
            }

            DevicesManagerMessage::SetDeviceFanMode { uuid, fan_mode } => {
                let device = self.devices.get_mut(&uuid).ok_or_else(|| {
                    DeviceManagerError::InvalidDevice {
                        reason: format!("Trying to access non-existing device"),
                    }
                })?;

                device.set_fan_mode(fan_mode)?;
            }
            DevicesManagerMessage::SetDeviceFanCurve { uuid, fan_curve } => {
                let device = self.devices.get_mut(&uuid).ok_or_else(|| {
                    DeviceManagerError::InvalidDevice {
                        reason: format!("Trying to access non-existing device"),
                    }
                })?;

                device.set_fan_curve(fan_curve);
            }
            DevicesManagerMessage::SetDeviceFanUpdateInterval {
                uuid,
                interval,
            } => {
                self.fan_update_intervals.insert(uuid, interval);
            }

            DevicesManagerMessage::ApplyDeviceGpuConfig { uuid, config } => {
                let device = self.devices.get_mut(&uuid).ok_or_else(|| {
                    DeviceManagerError::InvalidDevice {
                        reason: format!("Trying to access non-existing device"),
                    }
                })?;

                device.apply_gpu_config(config)?;
            }
        }

        Ok(())
    }

    // Return the duration until the next required fan update
    // also return the UUID of the device to update
    fn schedule_fan_update(&self) -> (String, Duration) {
        let mut smallest_delta = Duration::MAX;
        let mut update_device = String::new();

        for (uuid, last_update) in self.last_fan_updates.iter() {
            let interval = self.fan_update_intervals.get(uuid).unwrap().clone();

            // Time since the last update
            let elapsed = last_update.elapsed();
            // Time to the next update
            let delta = if interval > elapsed {
                interval - elapsed
            } else {
                Duration::from_secs(0)
            };

            if delta < smallest_delta {
                smallest_delta = delta;
                update_device = uuid.clone();
            }
        }

        (update_device, smallest_delta)
    }

    // Update the fans on the given device and update the last
    // fan update time
    fn update_fans(&mut self, uuid: &str) -> Result<()> {
        if let Some(device) = self.devices.get_mut(uuid) {
            device.update_fan()?;

            // Update last update time
            self.last_fan_updates
                .insert(uuid.to_string(), Instant::now());

            Ok(())
        } else {
            Err(DeviceManagerError::InvalidDevice {
                reason: format!(
                    "Trying to update fan on non-existing device: {}",
                    uuid
                ),
            })
        }
    }
}
