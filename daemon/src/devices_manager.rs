use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context, anyhow};
use nvml_wrapper::Nvml;
use tokio::time::Instant;
use tokio::{
    select,
    sync::{
        broadcast,
        mpsc::{Receiver, Sender},
        oneshot,
    },
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, trace, warn};

use crate::{
    config_manager::{ConfigMessage, ConfigMessageAnswer},
    fan_curve::{
        FanCurve, fan_curve_info::FanCurveInfo, fan_mode::FanMode,
        hysteresis_curve::HysteresisCurve, linear_curve::LinearCurve,
    },
    gpu_device::{
        DEFAULT_DATA_UPDATE_INTERVAL, DEFAULT_FAN_UPDATE_INTERVAL, GpuDevice,
        gpu_config::GpuConfig,
        gpu_data::{
            GpuData, GpuDataUpdates, GpuVendorData, GpuVendorDataUpdates,
        },
        gpu_info::{GpuInfo, GpuVendorInfo},
        nvidia_device::NvidiaDevice,
    },
};

type Responder = oneshot::Sender<DevicesManagerAnswer>;

// Alias the result type for this module
type Result<T> = std::result::Result<T, anyhow::Error>;

#[derive(Debug)]
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
        fan_curve: FanCurveInfo,
    },
    // Set the fan update interval for the device
    SetDeviceFanUpdateInterval {
        uuid: String,
        interval: Duration,
    },

    // Apply the given GPU configuration to the device
    SetDeviceConfig {
        uuid: String,
        config: GpuConfig,
    },

    // Query the config manager for configuration and apply it to
    // the given device
    ApplyConfigToDevice {
        uuid: String,
    },
    // Query the config manager for configuration and apply it to
    // all devices managed
    ApplyConfigToAllDevices,
}

#[derive(Debug)]
pub enum DevicesManagerAnswer {
    DeviceList(Vec<String>),

    DeviceInfo(GpuInfo),
    DeviceVendorInfo(GpuVendorInfo),

    DeviceData(Option<(GpuData, GpuDataUpdates)>),
    DeviceVendorData(Option<(GpuVendorData, GpuVendorDataUpdates)>),
}

#[derive(Debug, Clone)]
pub enum DeviceManagerNotification {
    // Notify of a device fan update, return the UUID of the updated device
    FanUpdated(String),

    // Notify of a device data update
    DataUpdated {
        uuid: String,

        data: GpuData,
        vendor_data: GpuVendorData,

        data_updates: GpuDataUpdates,
        vendor_data_updates: GpuVendorDataUpdates,
    },
}

pub struct DevicesManager {
    devices: HashMap<String, Box<dyn GpuDevice + Send>>,

    // Store the fan update interval for all the devices
    fan_update_intervals: HashMap<String, Duration>,
    // Store the last fan update instant for all the devices
    last_fan_updates: HashMap<String, Instant>,

    // Store the data update interval for all the devices
    data_update_intervals: HashMap<String, Duration>,
    // Store the last data update instant for all the devices
    last_data_updates: HashMap<String, Instant>,

    // Store the last device data and data updates
    last_data: HashMap<String, (GpuData, GpuDataUpdates)>,
    // Store the last device data and data updates
    last_vendor_data: HashMap<String, (GpuVendorData, GpuVendorDataUpdates)>,
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
                    "Error while updating fan speed on device manager creation: {}",
                    e
                )
            }

            last_fan_updates.insert(uuid.clone(), Instant::now());
            fan_update_intervals
                .insert(uuid.clone(), DEFAULT_FAN_UPDATE_INTERVAL);
        }

        // Create a hash map with the default data update interval
        // for each device
        // Create a hash map with the last data update instant
        let mut data_update_intervals = HashMap::new();
        let mut last_data_updates = HashMap::new();
        let mut last_data = HashMap::new();
        let mut last_vendor_data = HashMap::new();

        for (uuid, device) in devices.iter_mut() {
            // pull the data for the first time
            match device.get_data() {
                Ok((data, data_updates)) => {
                    debug!("Pulling data for: {}", uuid);
                    last_data.insert(uuid.clone(), (data, data_updates));
                }
                Err(e) => {
                    warn!(
                        "Error while pulling data on device manager creation: {}",
                        e
                    )
                }
            }
            match device.get_vendor_data() {
                Ok((vendor_data, vendor_data_updates)) => {
                    debug!("Pulling vendor data for: {}", uuid);
                    last_vendor_data.insert(
                        uuid.clone(),
                        (vendor_data, vendor_data_updates),
                    );
                }
                Err(e) => {
                    warn!(
                        "Error while pulling vendor data on device manager creation: {}",
                        e
                    )
                }
            }

            // Use the default update interval
            data_update_intervals
                .insert(uuid.clone(), DEFAULT_DATA_UPDATE_INTERVAL);
            last_data_updates.insert(uuid.clone(), Instant::now());
        }

        Self {
            devices,
            fan_update_intervals,
            last_fan_updates,

            data_update_intervals,
            last_data_updates,

            last_data,
            last_vendor_data,
        }
    }

    pub async fn run(
        &mut self,
        run_token: CancellationToken,

        mut rx_message: Receiver<DevicesManagerMessage>,
        tx_notificatioon: broadcast::Sender<DeviceManagerNotification>,

        tx_config: Sender<ConfigMessage>,
    ) {
        // Apply the configuration on manager startup
        self.apply_config_all_device(&tx_config)
            .await
            .unwrap_or_else(|e| {
                error!("Failed to apply config on device manager startup: {e}")
            });

        // Schedule the first fan and data update
        let (mut next_fan_update_device, mut next_fan_update_time) =
            self.schedule_fan_update();
        let (mut next_data_update_device, mut next_data_update_time) =
            self.schedule_data_update();

        loop {
            select! {
                _ = run_token.cancelled() => {
                    info!("devices manager: Quiting");

                    if let Err(err) = self.quit_manager() {
                        error!("Error while quitting devices manager: {}", err);
                    }

                    break;
                },
                message = rx_message.recv() => {
                    trace!("Handling message: {:?}", message);

                    self.parse_message(message, &tx_config).await.
                        unwrap_or_else(|e| error!("{e}"));
                },
                // Update the fan and schedule the next update
                _ = tokio::time::sleep_until(next_fan_update_time) => {
                    self.update_fans(&next_fan_update_device, &tx_notificatioon)
                        .unwrap_or_else(|e| error!("{e}"));

                    (next_fan_update_device, next_fan_update_time) =
                        self.schedule_fan_update();
                }
                // Update the data and schedule the next update
                _ = tokio::time::sleep_until(next_data_update_time) => {
                    self.update_data(&next_data_update_device, &tx_notificatioon)
                        .unwrap_or_else(|e| error!("{e}"));

                    (next_data_update_device, next_data_update_time) =
                        self.schedule_data_update();
                }
            }
        }
    }

    // Apply the config manager configuration to all manged devices
    async fn apply_config_all_device(
        &mut self,
        tx_config: &Sender<ConfigMessage>,
    ) -> Result<()> {
        for (uuid, device) in self.devices.iter_mut() {
            let fan_interval =
                if let Some(int) = self.fan_update_intervals.get_mut(uuid) {
                    int
                } else {
                    error!("Trying to update config for non-existant device!");
                    continue;
                };

            let data_interval =
                if let Some(int) = self.data_update_intervals.get_mut(uuid) {
                    int
                } else {
                    error!("Trying to update config for non-existant device!");
                    continue;
                };

            Self::apply_config(
                uuid,
                device,
                tx_config,
                fan_interval,
                data_interval,
            )
            .await?;
        }

        Ok(())
    }

    // Apply the config manager configuration for the given device
    async fn apply_config(
        uuid: &str,
        device: &mut Box<dyn GpuDevice + Send>,
        tx_config: &Sender<ConfigMessage>,
        fan_interval_ref: &mut Duration,
        data_interval_ref: &mut Duration,
    ) -> Result<()> {
        // Get the fan mode
        let (tx, rx) = oneshot::channel();
        let msg = ConfigMessage::GetDeviceFanMode {
            uuid: uuid.to_string(),
            tx,
        };

        tx_config.send(msg).await?;
        let fan_mode_answer = rx.await?;

        let fan_mode =
            if let ConfigMessageAnswer::FanMode(mode) = fan_mode_answer {
                mode
            } else {
                return Err(anyhow!("Wrong answer from config manager!"));
            };

        // Get the fan curve
        let (tx, rx) = oneshot::channel();
        let msg = ConfigMessage::GetDeviceFanCurve {
            uuid: uuid.to_string(),
            tx,
        };

        tx_config.send(msg).await?;

        let fan_curve_answer = rx.await?;

        let fan_curve =
            if let ConfigMessageAnswer::FanCurve(curve) = fan_curve_answer {
                curve
            } else {
                return Err(anyhow!("Wrong answer from config manager!"));
            };

        // Get the fan mode
        let (tx, rx) = oneshot::channel();
        let msg = ConfigMessage::GetDeviceConfig {
            uuid: uuid.to_string(),
            tx,
        };

        tx_config.send(msg).await?;
        let device_config_answer = rx.await?;

        let device_config = if let ConfigMessageAnswer::DeviceConfig(config) =
            device_config_answer
        {
            config
        } else {
            return Err(anyhow!("Wrong answer from config manager!"));
        };

        // Get fan update intervals
        let (tx, rx) = oneshot::channel();
        let msg = ConfigMessage::GetDeviceFanUpdateInterval {
            uuid: uuid.to_string(),
            tx,
        };

        tx_config.send(msg).await?;
        let fan_interval_answer = rx.await?;

        let fan_interval =
            if let ConfigMessageAnswer::FanUpdateInterval(interval) =
                fan_interval_answer
            {
                interval
            } else {
                return Err(anyhow!("Wrong answer from config manager!"));
            };

        // Get data update intervals
        let (tx, rx) = oneshot::channel();
        let msg = ConfigMessage::GetDeviceDataUpdateInterval {
            uuid: uuid.to_string(),
            tx,
        };

        tx_config.send(msg).await?;
        let data_interval_answer = rx.await?;

        let data_interval =
            if let ConfigMessageAnswer::DataUpdateInterval(interval) =
                data_interval_answer
            {
                interval
            } else {
                return Err(anyhow!("Wrong answer from config manager!"));
            };

        // Apply the configuration to the device
        // NOTE: the fan curve must be applied before the fan mode

        // Apply fan curve
        if let Some(info) = fan_curve {
            let curve_box = Self::get_fan_curve_from_info(info);
            device.set_fan_curve(curve_box);
        }
        device.set_fan_mode(fan_mode).unwrap_or_else(|e| error!("{}", e));

        // Apply the device config
        if let Some(config) = device_config {
            device.set_device_config(config).unwrap_or_else(|e| error!("{}", e));
        }

        // Apply the update intervals
        if let Some(int) = fan_interval {
            *fan_interval_ref = int;
        }
        if let Some(int) = data_interval {
            *data_interval_ref = int;
        }

        Ok(())
    }

    // Discover Nvidia GPUs on the system, create the associated
    // GPU devices and add them to the given hash map
    fn discover_nvidia_gpus(
        nvml: Arc<Nvml>,
        devices_map: &mut HashMap<String, Box<dyn GpuDevice + Send>>,
    ) -> Result<()> {
        let device_count = nvml
            .device_count()
            .with_context(|| "Failed to enumerate Nvidia devices")?;

        for i in 0..device_count {
            // Get the UUID of each device
            let device = nvml
                .device_by_index(i)
                .with_context(|| "Failed to get Nvidia device")?;

            let uuid = device.uuid().with_context(|| {
                format!("Failed to get Nvidia device uuid (index: {})", i)
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
    async fn parse_message(
        &mut self,
        message: Option<DevicesManagerMessage>,

        tx_config: &Sender<ConfigMessage>,
    ) -> Result<()> {
        if message.is_none() {
            return Ok(());
        }

        match message.unwrap() {
            DevicesManagerMessage::ListDevices { tx } => {
                let mut devices_list = Vec::new();

                for (uuid, _) in self.devices.iter() {
                    devices_list.push(uuid.clone());
                }

                let answer = DevicesManagerAnswer::DeviceList(devices_list);
                tx.send(answer).map_err(|v| {
                    anyhow!(format!(
                        "Failed to send answer over channel: ({:?})",
                        v
                    ))
                })?;
            }

            DevicesManagerMessage::GetDeviceInfo { uuid, tx } => {
                let device = self.devices.get(&uuid).ok_or_else(|| {
                    anyhow!("Trying to access non-existing device")
                })?;

                let answer =
                    DevicesManagerAnswer::DeviceInfo(device.get_info());
                tx.send(answer).map_err(|v| {
                    anyhow!(format!(
                        "Failed to send answer over channel: ({:?})",
                        v
                    ))
                })?
            }
            DevicesManagerMessage::GetDeviceVendorInfo { uuid, tx } => {
                let device = self.devices.get(&uuid).ok_or_else(|| {
                    anyhow!(format!("Trying to access non-existing device"))
                })?;

                let answer = DevicesManagerAnswer::DeviceVendorInfo(
                    device.get_vendor_info(),
                );
                tx.send(answer).map_err(|v| {
                    anyhow!(format!(
                        "Failed to send answer over channel: ({:?})",
                        v
                    ))
                })?
            }

            DevicesManagerMessage::GetDeviceData { uuid, tx } => {
                // Generate answer
                let data = self.last_data.get(&uuid);
                let answer = DevicesManagerAnswer::DeviceData(data.cloned());

                tx.send(answer).map_err(|v| {
                    anyhow!(format!(
                        "Failed to send answer over channel: ({:?})",
                        v
                    ))
                })?
            }
            DevicesManagerMessage::GetDeviceVendorData { uuid, tx } => {
                // Generate answer
                let vendor_data = self.last_vendor_data.get(&uuid);
                let answer = DevicesManagerAnswer::DeviceVendorData(
                    vendor_data.cloned(),
                );

                tx.send(answer).map_err(|v| {
                    anyhow!(format!(
                        "Failed to send answer over channel: ({:?})",
                        v
                    ))
                })?
            }
            DevicesManagerMessage::SetDeviceDataUpdateInterval {
                uuid,
                interval,
            } => {
                let interval_ref = self.data_update_intervals.get_mut(&uuid);
                if let Some(intr) = interval_ref {
                    *intr = interval;
                } else {
                    warn!(
                        "Attempting data interval change on non-initialized GPU"
                    );
                    self.data_update_intervals.insert(uuid.clone(), interval);
                }
            }

            DevicesManagerMessage::SetDeviceFanMode { uuid, fan_mode } => {
                let device = self.devices.get_mut(&uuid).ok_or_else(|| {
                    anyhow!("Trying to access non-existing device")
                })?;

                device.set_fan_mode(fan_mode)?;
            }
            DevicesManagerMessage::SetDeviceFanCurve { uuid, fan_curve } => {
                let device = self.devices.get_mut(&uuid).ok_or_else(|| {
                    anyhow!("Trying to access non-existing device")
                })?;

                // Generate the fan curve
                let curve_box = Self::get_fan_curve_from_info(fan_curve);

                device.set_fan_curve(curve_box);
            }
            DevicesManagerMessage::SetDeviceFanUpdateInterval {
                uuid,
                interval,
            } => {
                self.fan_update_intervals.insert(uuid, interval);
            }

            DevicesManagerMessage::SetDeviceConfig { uuid, config } => {
                let device = self.devices.get_mut(&uuid).ok_or_else(|| {
                    anyhow!("Trying to access non-existing device")
                })?;

                device.set_device_config(config)?;
            }

            DevicesManagerMessage::ApplyConfigToDevice { uuid } => {
                let device = self.devices.get_mut(&uuid).ok_or_else(|| {
                    anyhow!("Trying to access non-existing device")
                })?;

                let fan_interval = if let Some(int) =
                    self.fan_update_intervals.get_mut(&uuid)
                {
                    int
                } else {
                    error!("Trying to update config for non-existant device!");
                    return Ok(());
                };

                let data_interval = if let Some(int) =
                    self.data_update_intervals.get_mut(&uuid)
                {
                    int
                } else {
                    error!("Trying to update config for non-existant device!");
                    return Ok(());
                };

                Self::apply_config(
                    &uuid,
                    device,
                    tx_config,
                    fan_interval,
                    data_interval,
                )
                .await?;
            }
            DevicesManagerMessage::ApplyConfigToAllDevices => {
                self.apply_config_all_device(tx_config).await?;
            }
        }

        Ok(())
    }

    // Return the instant until the next required fan update
    // also return the UUID of the device to update
    fn schedule_fan_update(&self) -> (String, Instant) {
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

        // Calculate the instant until we have to sleep
        let update_time = Instant::now() + smallest_delta;

        (update_device, update_time)
    }

    // Return the instant until the next required data update
    // also return the UUID of the device to update
    fn schedule_data_update(&self) -> (String, Instant) {
        let mut smallest_delta = Duration::MAX;
        let mut update_device = String::new();

        for (uuid, last_update) in self.last_data_updates.iter() {
            let interval =
                self.data_update_intervals.get(uuid).unwrap().clone();

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

        // Calculate the instant until we have to sleep
        let update_time = Instant::now() + smallest_delta;

        (update_device, update_time)
    }

    // Update the fans on the given device and update the last
    // fan update time
    fn update_fans(
        &mut self,
        uuid: &str,
        tx_notificatioon: &broadcast::Sender<DeviceManagerNotification>,
    ) -> Result<()> {
        if let Some(device) = self.devices.get_mut(uuid) {
            device.update_fan()?;

            // Update last update time
            self.last_fan_updates
                .insert(uuid.to_string(), Instant::now());

            // Send update notification
            let notification =
                DeviceManagerNotification::FanUpdated(uuid.to_string());
            if let Err(_) = tx_notificatioon.send(notification) {
                warn!("Failed to send FanUpdated notification");
            }

            Ok(())
        } else {
            Err(anyhow!(format!(
                "Trying to update fan on non-existing device: {}",
                uuid
            )))
        }
    }

    // Update the data on the given device and update the last
    // data update time
    fn update_data(
        &mut self,
        uuid: &str,
        tx_notificatioon: &broadcast::Sender<DeviceManagerNotification>,
    ) -> Result<()> {
        if let Some(device) = self.devices.get_mut(uuid) {
            let data = device.get_data()?;
            let vendor_data = device.get_vendor_data()?;

            self.last_data.insert(uuid.to_string(), data);
            self.last_vendor_data.insert(uuid.to_string(), vendor_data);

            // Update last update time
            self.last_data_updates
                .insert(uuid.to_string(), Instant::now());

            // Send update notification
            let notification = DeviceManagerNotification::DataUpdated {
                uuid: uuid.to_string(),
                data: data.0,
                vendor_data: vendor_data.0,
                data_updates: data.1,
                vendor_data_updates: vendor_data.1,
            };
            if let Err(_) = tx_notificatioon.send(notification) {
                warn!("Failed to send DataUpdated notification");
            }

            Ok(())
        } else {
            Err(anyhow!(format!(
                "Trying to update data on non-existing device: {}",
                uuid
            )))
        }
    }

    // Restore the default setting for all device before quitting
    fn quit_manager(&mut self) -> Result<()> {
        for (_, device) in self.devices.iter_mut() {
            device.set_fan_mode(FanMode::Auto)?;
            device.set_device_config(GpuConfig::default())?;
        }

        Ok(())
    }

    // Generate a linear fan curve with the given info
    fn get_fan_curve_from_info(info: FanCurveInfo) -> Box<dyn FanCurve + Send> {
        Box::new(HysteresisCurve::<LinearCurve>::from_info(&info))
    }
}
