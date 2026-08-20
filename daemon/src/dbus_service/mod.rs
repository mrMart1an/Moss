mod config_interface;
mod fan_curve_interface;
mod gpu_interface;
mod log_interface;
mod nvidia_interface;
mod profile_interface;
mod profile_nvidia_interface;

use std::collections::HashMap;

use anyhow::{Context, anyhow};
use tokio::{
    select,
    sync::{
        broadcast,
        mpsc::{Receiver, Sender},
        oneshot,
    },
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use zbus::{
    Connection,
    fdo::ObjectManager,
    object_server::{Interface, InterfaceRef, SignalEmitter},
};

use crate::{
    config,
    config_manager::{ConfigMessage, ConfigMessageAnswer},
    dbus_service::{
        config_interface::ConfigInterface,
        fan_curve_interface::FanCurveInterface,
        gpu_interface::{GpuInterface, GpuInterfaceSignals},
        log_interface::{LogInterface, LogInterfaceSignals},
        nvidia_interface::NvidiaInterface,
        profile_interface::ProfileInterface,
        profile_nvidia_interface::ProfileNvidiaInterface,
    },
    devices_manager::{
        DeviceManagerNotification, DevicesManagerAnswer, DevicesManagerMessage,
    },
    gpu_device::{
        gpu_data::{GpuDataUpdates, GpuVendorDataUpdates},
        gpu_info::GpuVendorInfo,
    },
    logger::{dbus_layer::DBusLog, level_to_u8},
};

#[macro_export]
macro_rules! extract_answer {
    ( $expected:path, $answer:expr ) => {{
        let result = if let $expected(data) = $answer {
            Ok(data)
        } else {
            Err(anyhow!(format!("Invalid responce {:?}", $answer)))
        };

        result
    }};
}

// Convert an Option to a D-Bus friendly tuple
pub fn opt_dbus<T: Default>(opt: Option<T>) -> (bool, T) {
    if let Some(data) = opt {
        (true, data)
    } else {
        (false, T::default())
    }
}
// Convert a D-Bus friendly tuple to an Option
pub fn dbus_opt<T>(opt: (bool, T)) -> Option<T> {
    if opt.0 { Some(opt.1) } else { None }
}

const LOG_OBJECT_PATH: &str = "/com/github/Mossd1/Log";

const CONFIG_OBJECT_PATH: &str = "/com/github/Mossd1/Config";
const PROFILE_OBJECT_SUBPATH: &str = "/Profiles";
const FAN_CURVE_OBJECT_SUBPATH: &str = "/FanCurves";

type Result<T> = std::result::Result<T, anyhow::Error>;

pub struct DBusService {
    log_object_path: &'static str,

    // Store the D-Bus path corresponding to each device UUID
    device_dbus_path: HashMap<String, String>,
}

impl DBusService {
    pub fn new() -> Self {
        Self {
            log_object_path: LOG_OBJECT_PATH,
            device_dbus_path: HashMap::new(),
        }
    }

    pub async fn run(
        &mut self,
        run_token: CancellationToken,

        tx_device_manager: Sender<DevicesManagerMessage>,
        tx_config_manager: Sender<ConfigMessage>,

        mut rx_devices_notification: broadcast::Receiver<
            DeviceManagerNotification,
        >,
        mut rx_dbus_log: Receiver<DBusLog>,
    ) {
        // Connect to the system D-Bus
        let connection = match Connection::system().await {
            Ok(conn) => conn,
            Err(err) => {
                error!("Failed to establish connection with the bus: {}", err);

                // Just return, there is nothing else to do
                return;
            }
        };

        info!("DBus connection enstablished");

        // Initialize the GPUs interfaces
        if let Err(err) = self
            .initialize_gpus_objects(&connection, tx_device_manager.clone())
            .await
        {
            error!("{}", err);
        }

        // Initialize the configuration interface
        if let Err(err) = self
            .initialize_config_objects(
                &connection,
                tx_device_manager,
                tx_config_manager,
            )
            .await
        {
            error!("{}", err);
        }

        // Request the service name
        // NOTE:    The name request must happen AFTER setting up the
        //          server object or messages might be lost
        if let Err(err) = connection
            .request_name(config::service_name())
            .await
            .with_context(|| "Failed to acquire service name")
        {
            error!("{}", err);
        }

        loop {
            select! {
                _ = run_token.cancelled() => {
                    info!("DBus service: Quiting");
                    break;
                },
                log = rx_dbus_log.recv() => {
                    if let Err(e) = Self::send_log_singal(&connection, log).await {
                        error!("Failed to send log signal: {}", e);
                    }
                }
                notification_res = rx_devices_notification.recv() => {
                    let notification = if let Ok(notif) = notification_res {
                        notif
                    } else {
                        continue;
                    };

                    if let Err(e) = self.send_notification_signals(&connection, notification).await {
                        error!("Failed to send notificatioin signal: {}", e);
                    }
                }
            }
        }
    }

    async fn initialize_config_objects(
        &mut self,

        connection: &Connection,

        tx_device_manager: Sender<DevicesManagerMessage>,
        tx_config_manager: Sender<ConfigMessage>,
    ) -> Result<()> {
        // Get the Profiles
        let (tx, rx) = oneshot::channel();
        let message = ConfigMessage::ListProfiles { tx };

        tx_config_manager.send(message).await?;
        let answer = rx.await?;

        let profiles_list =
            extract_answer!(ConfigMessageAnswer::ProfilesList, answer)?;

        // Get the Fan Curves
        let (tx, rx) = oneshot::channel();
        let message = ConfigMessage::ListFanCurves { tx };

        tx_config_manager.send(message).await?;
        let answer = rx.await?;

        let fan_curves_list =
            extract_answer!(ConfigMessageAnswer::FanCurvesList, answer)?;

        // Generate the configuration object
        let config_object = ConfigInterface::new(
            connection.clone(),
            tx_device_manager,
            tx_config_manager.clone(),
            profiles_list.clone(),
            fan_curves_list.clone(),
        );

        connection
            .object_server()
            .at(CONFIG_OBJECT_PATH, config_object)
            .await?;

        // Create the object manager for the config objects
        connection
            .object_server()
            .at(format!("{}", CONFIG_OBJECT_PATH), ObjectManager)
            .await?;

        // Generate the already existing configuration objects

        // Generate the profiles objects
        for profile in profiles_list.iter() {
            // Generate profile interface
            let profile_interface = ProfileInterface::new(
                profile.clone(),
                tx_config_manager.clone(),
            );
            connection
                .object_server()
                .at(
                    format!(
                        "{}{}/{}",
                        CONFIG_OBJECT_PATH, PROFILE_OBJECT_SUBPATH, profile
                    ),
                    profile_interface,
                )
                .await?;

            // Generate profile Nvidia interface
            let profile_nvidia_interface = ProfileNvidiaInterface::new(
                profile.clone(),
                tx_config_manager.clone(),
            );
            connection
                .object_server()
                .at(
                    format!(
                        "{}{}/{}",
                        CONFIG_OBJECT_PATH, PROFILE_OBJECT_SUBPATH, profile
                    ),
                    profile_nvidia_interface,
                )
                .await?;
        }

        // Generate the fan curves objects
        for curve in fan_curves_list.iter() {
            // Generate the fan curve interface
            let fan_curve_interface = FanCurveInterface::new(
                curve.clone(),
                tx_config_manager.clone(),
            );
            connection
                .object_server()
                .at(
                    format!(
                        "{}{}/{}",
                        CONFIG_OBJECT_PATH, FAN_CURVE_OBJECT_SUBPATH, curve
                    ),
                    fan_curve_interface,
                )
                .await?;
        }

        Ok(())
    }

    async fn initialize_gpus_objects(
        &mut self,
        connection: &Connection,

        tx_device_manager: Sender<DevicesManagerMessage>,
    ) -> Result<()> {
        // Initialize the error interface
        self.initialize_log_object(connection).await?;

        // Query the state manager to get a list of the available GPUs
        let (tx, rx) = oneshot::channel();
        let message = DevicesManagerMessage::ListDevices { tx };

        tx_device_manager.send(message).await?;
        let answer = rx.await?;

        let gpu_uuids = if let DevicesManagerAnswer::DeviceList(uuids) = answer
        {
            Ok(uuids)
        } else {
            Err(anyhow!(format!("Invalid response from state manager")))
        }?;

        // Create a D-Bus object for each GPUs on the system
        let mut gpu_count = 1;

        for uuid in gpu_uuids {
            debug!("Creating D-Bus object for GPU: {}", uuid);

            let path = format!("/com/github/Mossd1/Gpu{}", gpu_count);

            // Store the D-Bus path for the device
            self.device_dbus_path.insert(uuid.clone(), path.clone());

            Self::initialize_gpu_object(
                path,
                uuid,
                connection,
                tx_device_manager.clone(),
            )
            .await?;

            gpu_count += 1;
        }

        Ok(())
    }

    async fn initialize_log_object(
        &mut self,
        connection: &Connection,
    ) -> Result<()> {
        connection
            .object_server()
            .at(self.log_object_path, LogInterface::default())
            .await
            .with_context(|| "Error while initializing log object")?;

        Ok(())
    }

    async fn initialize_gpu_object(
        path: String,
        uuid: String,

        connection: &Connection,
        tx_device_manager: Sender<DevicesManagerMessage>,
    ) -> Result<()> {
        // Get the GPU vendor infos
        let (tx, rx) = oneshot::channel();
        let message = DevicesManagerMessage::GetDeviceVendorInfo {
            uuid: uuid.clone(),
            tx,
        };

        tx_device_manager.send(message).await?;
        let answer = rx.await?;

        let gpu_vendor_info =
            extract_answer!(DevicesManagerAnswer::DeviceVendorInfo, answer)?;

        connection
            .object_server()
            .at(
                path.clone(),
                GpuInterface::new(uuid.clone(), tx_device_manager.clone())
                    .await?,
            )
            .await
            .with_context(|| "Error while initializing GPU object")?;

        // Create a Nvidia interface if the GPU is Nvidia
        if matches!(gpu_vendor_info, GpuVendorInfo::Nvidia { .. }) {
            connection
                .object_server()
                .at(
                    path.clone(),
                    NvidiaInterface::new(
                        uuid.clone(),
                        gpu_vendor_info,
                        tx_device_manager.clone(),
                    )
                    .await?,
                )
                .await
                .with_context(
                    || "Error while initializing vendor GPU object",
                )?;
        }

        Ok(())
    }

    async fn send_notification_signals(
        &mut self,
        connection: &Connection,
        notification: DeviceManagerNotification,
    ) -> Result<()> {
        match notification {
            DeviceManagerNotification::FanUpdated(uuid) => {
                let path = self.device_dbus_path.get(&uuid);

                if let Some(path) = path {
                    // Get the interface and send the fan update signal signal
                    let interface =
                        Self::get_dbus_interface_ref::<GpuInterface>(
                            connection, path,
                        )
                        .await?;

                    interface.fan_update().await?;
                } else {
                    warn!("Fan updated for non existat GPU");
                }
            }
            DeviceManagerNotification::DataUpdated {
                uuid,
                data: _data,
                vendor_data: _vendor_data,
                data_updates,
                vendor_data_updates,
            } => {
                // TODO:
                let path = self.device_dbus_path.get(&uuid);

                // Send GPU data update signal
                if let Some(path) = path {
                    Self::send_data_update_signal(
                        connection,
                        path,
                        data_updates,
                    )
                    .await?;
                } else {
                    warn!("Data updated for non existat GPU");
                }

                // Send GPU vendor data update signal
                if let Some(path) = path {
                    Self::send_vendor_data_update_signal(
                        connection,
                        path,
                        vendor_data_updates,
                    )
                    .await?;
                } else {
                    warn!("Vendor data updated for non existat GPU");
                }
            }
        }

        Ok(())
    }

    async fn send_data_update_signal(
        connection: &Connection,
        path: &str,
        data_updates: GpuDataUpdates,
    ) -> Result<()> {
        // Get the GPU interface
        let interface_ref =
            Self::get_dbus_interface_ref::<GpuInterface>(connection, path)
                .await?;
        let interface = interface_ref.get().await;

        // Create signal emitter
        let emitter = SignalEmitter::new(connection, path)?;

        // This will return an error if a field is added to the struct
        let GpuDataUpdates {
            temp_gpu,
            graphics_freq,
            mem_freq,
            core_clock_offset,
            mem_clock_offset,
            power_usage,
            power_limit,
            fan_speed,
            fan_speed_rpm,
            core_usage,
            mem_usage,
            total_memory,
            used_memory,
            free_memory,
        } = data_updates;

        // Send the signals
        if temp_gpu {
            interface.temperature_changed(&emitter).await?;
        }
        if free_memory {
            interface.free_memory_changed(&emitter).await?;
        }
        if fan_speed {
            interface.fan_speed_changed(&emitter).await?;
        }
        if power_limit {
            interface.power_limit_changed(&emitter).await?;
        }
        if power_usage {
            interface.power_usage_changed(&emitter).await?;
        }
        if mem_clock_offset {
            interface.memory_clock_offset_changed(&emitter).await?;
        }
        if fan_speed_rpm {
            interface.fan_speed_rpm_changed(&emitter).await?;
        }
        if mem_usage {
            interface.memory_usage_changed(&emitter).await?;
        }
        if core_usage {
            interface.core_usage_changed(&emitter).await?;
        }
        if total_memory {
            interface.total_memory_changed(&emitter).await?;
        }
        if used_memory {
            interface.used_memory_changed(&emitter).await?;
        }
        if core_clock_offset {
            interface.core_clock_offset_changed(&emitter).await?;
        }
        if graphics_freq {
            interface.graphics_frequency_changed(&emitter).await?;
        }
        if mem_freq {
            interface.memory_frequency_changed(&emitter).await?;
        }

        Ok(())
    }

    async fn send_vendor_data_update_signal(
        connection: &Connection,
        path: &str,
        vendor_data_updates: GpuVendorDataUpdates,
    ) -> Result<()> {
        // Create signal emitter
        let emitter = SignalEmitter::new(connection, path)?;

        match vendor_data_updates {
            GpuVendorDataUpdates::Nvidia {
                sm_freq,
                video_freq,
                graphics_boost_freq,
                mem_boost_freq,
                sm_boost_freq,
                video_boost_freq,
            } => {
                // Get the GPU interface
                let interface_ref = Self::get_dbus_interface_ref::<
                    NvidiaInterface,
                >(connection, path)
                .await?;
                let interface = interface_ref.get().await;

                // Send the signals
                if sm_freq {
                    interface.sm_frequency_changed(&emitter).await?;
                }
                if video_freq {
                    interface.video_frequency_changed(&emitter).await?;
                }
                if graphics_boost_freq {
                    interface.graphic_boost_frequency_changed(&emitter).await?;
                }
                if mem_boost_freq {
                    interface.memory_boost_frequency_changed(&emitter).await?;
                }
                if sm_boost_freq {
                    interface.sm_boost_frequency_changed(&emitter).await?;
                }
                if video_boost_freq {
                    interface.video_boost_frequency_changed(&emitter).await?;
                }
            }
            _ => {
                error!("Unimplemented Vendor data update signals");
            }
        }

        Ok(())
    }

    // Send the Log signal on the logging interface
    async fn send_log_singal(
        connection: &Connection,
        log_opt: Option<DBusLog>,
    ) -> Result<()> {
        let log = if let Some(log) = log_opt {
            log
        } else {
            return Ok(());
        };

        let interface = Self::get_dbus_interface_ref::<LogInterface>(
            connection,
            LOG_OBJECT_PATH,
        )
        .await?;

        // Prepare signal
        let level = level_to_u8(log.level);

        let file = log.file.unwrap_or_else(|| format!("<unknow file>"));
        let line = if let Some(line) = log.line {
            line as i32
        } else {
            -1
        };

        let message = log.message;

        // Send signal
        if let Err(err) = interface.new_log(level, file, line, &message).await {
            warn!("Failed to generate log signal: {}", err);
        }

        Ok(())
    }

    async fn get_dbus_interface_ref<T: Interface>(
        connection: &Connection,
        path: &str,
    ) -> Result<InterfaceRef<T>> {
        let obj_server = connection.object_server();

        obj_server
            .interface::<_, T>(path)
            .await
            .with_context(|| "Failed to get object interface")
    }
}
