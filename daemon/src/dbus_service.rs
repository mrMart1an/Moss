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
use tracing::{debug, error, info, trace, warn};
use zbus::{
    Connection, interface,
    object_server::{Interface, InterfaceRef, SignalEmitter},
};

use crate::{
    config_manager::ConfigMessage,
    devices_manager::{
        DeviceManagerNotification, DevicesManagerAnswer, DevicesManagerMessage,
    },
    gpu_device::{
        gpu_data::{GpuData, GpuVendorData},
        gpu_info::{GpuInfo, GpuVendorInfo},
    },
    logger::{dbus_layer::DBusLog, level_to_u8},
};

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

const SERVICE_NAME: &str = "com.github.Mossd1";
const LOG_OBJECT_PATH: &str = "/com/github/Mossd1/Log";

type Result<T> = std::result::Result<T, anyhow::Error>;

pub struct DBusService {
    log_object_path: &'static str,

    // Store the D-Bus path corresponding to each device UUID
    device_dbus_path: HashMap<String, String>,
}

// Interface of the D-Bus service used for error reporting
#[derive(Default)]
struct LogInterface {}

// GPU D-Bus interface
struct GpuInterface {
    uuid: String,

    tx_device_manager: Sender<DevicesManagerMessage>,

    gpu_info: GpuInfo,
}

struct NvidiaInterface {
    uuid: String,

    tx_device_manager: Sender<DevicesManagerMessage>,

    gpu_vendor_info: GpuVendorInfo,
}

#[interface(name = "com.github.Mossd1.Log")]
impl LogInterface {
    #[zbus(signal)]
    pub async fn new_log(
        emitter: &SignalEmitter<'_>,

        level: u8,

        file: String,
        line: i32,

        error: &str,
    ) -> zbus::Result<()>;
}

#[interface(name = "com.github.Mossd1.Gpu")]
impl GpuInterface {
    // GPU info properties
    #[zbus(property)]
    async fn uuid(&self) -> &str {
        &self.uuid
    }
    #[zbus(property)]
    async fn name(&self) -> &str {
        &self.gpu_info.name
    }

    #[zbus(property)]
    async fn pcie_width(&self) -> u32 {
        self.gpu_info.pcie_width
    }
    #[zbus(property)]
    async fn pcie_gen(&self) -> u32 {
        self.gpu_info.pcie_gen
    }

    #[zbus(property)]
    async fn power_limit_max(&self) -> u32 {
        self.gpu_info.power_limit_max
    }
    #[zbus(property)]
    async fn power_limit_min(&self) -> u32 {
        self.gpu_info.power_limit_min
    }
    #[zbus(property)]
    async fn power_limit_default(&self) -> u32 {
        self.gpu_info.power_limit_default
    }

    // GPU data properties
    #[zbus(property)]
    async fn temperature(&self) -> zbus::fdo::Result<u32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.temp_gpu)
    }

    #[zbus(property)]
    async fn graphics_frequency(&self) -> zbus::fdo::Result<u32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.graphics_freq)
    }
    #[zbus(property)]
    async fn memory_frequency(&self) -> zbus::fdo::Result<u32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.mem_freq)
    }

    #[zbus(property)]
    async fn core_clock_offset(&self) -> zbus::fdo::Result<i32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.core_clock_offset)
    }
    #[zbus(property)]
    async fn memory_clock_offset(&self) -> zbus::fdo::Result<i32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.mem_clock_offset)
    }

    #[zbus(property)]
    async fn power_usage(&self) -> zbus::fdo::Result<u32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.power_usage)
    }
    #[zbus(property)]
    async fn power_limit(&self) -> zbus::fdo::Result<u32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.power_limit)
    }

    #[zbus(property)]
    async fn fan_speed(&self) -> zbus::fdo::Result<u32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.fan_speed)
    }
    #[zbus(property)]
    async fn fan_speed_rpm(&self) -> zbus::fdo::Result<u32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.fan_speed_rpm)
    }

    #[zbus(property)]
    async fn core_usage(&self) -> zbus::fdo::Result<u32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.core_usage)
    }
    #[zbus(property)]
    async fn memory_usage(&self) -> zbus::fdo::Result<u32> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.mem_usage)
    }

    #[zbus(property)]
    async fn total_memory(&self) -> zbus::fdo::Result<u64> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.total_memory)
    }
    #[zbus(property)]
    async fn used_memory(&self) -> zbus::fdo::Result<u64> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.used_memory)
    }
    #[zbus(property)]
    async fn free_memory(&self) -> zbus::fdo::Result<u64> {
        let data = self
            .get_data()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("{:?}", e)))?;
        Ok(data.free_memory)
    }

    // Fan update signal
    #[zbus(signal)]
    pub async fn fan_update(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

#[interface(name = "com.github.Mossd1.Nvidia")]
impl NvidiaInterface {
    // GPU vendor info properties
    #[zbus(property)]
    async fn driver_version(&self) -> &str {
        if let GpuVendorInfo::Nvidia { driver_version, .. } =
            &self.gpu_vendor_info
        {
            driver_version
        } else {
            &"VENDOR INFO NOT NVIDIA!"
        }
    }
    #[zbus(property)]
    async fn vbios(&self) -> &str {
        if let GpuVendorInfo::Nvidia { vbios, .. } = &self.gpu_vendor_info {
            vbios
        } else {
            &"VENDOR INFO NOT NVIDIA!"
        }
    }

    #[zbus(property)]
    async fn cuda_core_count(&self) -> u32 {
        if let GpuVendorInfo::Nvidia {
            cuda_core_count, ..
        } = self.gpu_vendor_info
        {
            cuda_core_count
        } else {
            0
        }
    }

    #[zbus(property)]
    async fn max_temp(&self) -> u32 {
        if let GpuVendorInfo::Nvidia { max_temp, .. } = self.gpu_vendor_info {
            max_temp.unwrap_or(0)
        } else {
            0
        }
    }
    #[zbus(property)]
    async fn mem_max_temp(&self) -> u32 {
        if let GpuVendorInfo::Nvidia { mem_max_temp, .. } = self.gpu_vendor_info
        {
            mem_max_temp.unwrap_or(0)
        } else {
            0
        }
    }
    #[zbus(property)]
    async fn slowdown_temp(&self) -> u32 {
        if let GpuVendorInfo::Nvidia { slowdown_temp, .. } =
            self.gpu_vendor_info
        {
            slowdown_temp.unwrap_or(0)
        } else {
            0
        }
    }
    #[zbus(property)]
    async fn shutdown_temp(&self) -> u32 {
        if let GpuVendorInfo::Nvidia { shutdown_temp, .. } =
            self.gpu_vendor_info
        {
            shutdown_temp.unwrap_or(0)
        } else {
            0
        }
    }
}

impl GpuInterface {
    async fn new(
        uuid: String,
        tx_device_manager: Sender<DevicesManagerMessage>,
    ) -> Result<Self> {
        // Get the GPU infos
        let (tx, rx) = oneshot::channel();
        let message = DevicesManagerMessage::GetDeviceInfo {
            uuid: uuid.clone(),
            tx,
        };

        tx_device_manager.send(message).await?;
        let answer = rx.await?;

        let gpu_info =
            extract_answer!(DevicesManagerAnswer::DeviceInfo, answer)?;

        Ok(Self {
            uuid,
            tx_device_manager,
            gpu_info,
        })
    }

    async fn get_data(&self) -> Result<GpuData> {
        // Get the GPU infos
        let (tx, rx) = oneshot::channel();
        let message = DevicesManagerMessage::GetDeviceData {
            uuid: self.uuid.clone(),
            tx,
        };

        self.tx_device_manager.send(message).await?;
        let answer = rx.await?;

        let gpu_data =
            extract_answer!(DevicesManagerAnswer::DeviceData, answer)?;

        // Return an error if no data was provided by the manager
        if let Some(data) = gpu_data {
            Ok(data.0)
        } else {
            Err(anyhow!("Manager failed to provide device data"))
        }
    }
}

impl NvidiaInterface {
    async fn new(
        uuid: String,
        gpu_vendor_info: GpuVendorInfo,
        tx_device_manager: Sender<DevicesManagerMessage>,
    ) -> Result<Self> {
        Ok(Self {
            uuid,

            tx_device_manager,

            gpu_vendor_info,
        })
    }
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

        if let Err(err) = self
            .initialize_service(&connection, tx_device_manager)
            .await
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
                data,
                vendor_data,
                data_updates,
                vendor_data_updates,
            } => {
                // TODO:
            }
        }

        Ok(())
    }

    async fn initialize_service(
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

        // Request the service name
        // NOTE:    The name request must happen AFTER setting up the
        //          server object or messages might be lost
        connection
            .request_name(SERVICE_NAME)
            .await
            .with_context(|| "Failed to acquire service name")?;

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
