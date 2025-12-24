use thiserror::Error;
use tokio::{
    select,
    sync::{
        mpsc::{Receiver, Sender},
        oneshot,
    },
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, trace};
use zbus::{
    Connection, interface,
    object_server::{Interface, InterfaceRef, SignalEmitter},
};

use crate::{
    errors::MossdError,
    gpu_device::gpu_info::{GpuInfo, GpuVendorInfo},
    state_manager::{StateManagerAnswer, StateManagerMessage},
};

macro_rules! extract_answer {
    ( $expected:path, $answer:expr ) => {{
        let result = if let $expected(data) = $answer {
            Ok(data)
        } else {
            Err(DbusServiceError::InvalidResponse {
                reason: format!("Invalid responce {:?}", $answer),
            })
        };

        result
    }};
}

const SERVICE_NAME: &str = "com.github.Mossd1";
const ERROR_OBJECT_PATH: &str = "/com/github/Mossd1/Error";

type Responder = oneshot::Sender<DBusServiceAnswer>;

type Result<T> = std::result::Result<T, DbusServiceError>;

#[derive(Debug, Error)]
pub enum DbusServiceError {
    #[error("DBus service manager TX error: {reason}")]
    TX { reason: String },
    #[error("DBus service manager RX error: {reason}")]
    RX {
        reason: String,
        error: anyhow::Error,
    },
    #[error("DBus service invalid response error: {reason}")]
    InvalidResponse { reason: String },
    #[error("DBus service DBus connection error: {reason}")]
    DBusConnection {
        reason: String,
        error: anyhow::Error,
    },
    #[error("DBus service DBus object error: {reason}")]
    DBusObject {
        reason: String,
        error: anyhow::Error,
    },
    #[error("DBus service DBus signal error: {reason}")]
    DBusSignal {
        reason: String,
        error: anyhow::Error,
    },
}

// This is the message enum that the state manager process will
// send to the D-Bus service to notify it
pub enum DBusServiceMessage {
    // Notify the D-Bus service of an error in the daemon
    NewError(MossdError),
}

#[derive(Debug)]
pub enum DBusServiceAnswer {}

pub struct DBusService {
    error_object_path: &'static str,
}

// Interface of the D-Bus service used for error reporting
#[derive(Default)]
struct ErrorInterface {}

#[interface(name = "com.github.Mossd1.Error")]
impl ErrorInterface {
    #[zbus(signal)]
    pub async fn new_error(
        emitter: &SignalEmitter<'_>,
        error_code: u32,
        error: &str,
    ) -> zbus::Result<()>;
}

// GPU D-Bus interface
struct GpuInterface {
    uuid: String,

    tx_dbus_to_manager: Sender<StateManagerMessage>,
    tx_err: Sender<MossdError>,

    gpu_info: GpuInfo,
}

impl GpuInterface {
    async fn new(
        uuid: String,
        tx_dbus_to_manager: Sender<StateManagerMessage>,
        tx_err: Sender<MossdError>,
    ) -> Result<Self> {
        // Get the GPU infos
        let (tx, rx) = oneshot::channel();
        let message = StateManagerMessage::GetGpuInfo {
            uuid: uuid.clone(),
            tx,
        };

        tx_dbus_to_manager.send(message).await.map_err(|_| {
            DbusServiceError::TX {
                reason: format!("Failed to send message to state manager"),
            }
        })?;

        let answer = rx.await.map_err(|e| DbusServiceError::RX {
            reason: format!("Failed to receive answer from state manager"),
            error: e.into(),
        })?;

        let gpu_info = extract_answer!(StateManagerAnswer::GpuInfo, answer)?;

        Ok(Self {
            uuid,

            tx_dbus_to_manager,
            tx_err,

            gpu_info,
        })
    }
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
}

struct NvidiaInterface {
    uuid: String,

    tx_dbus_service: Sender<StateManagerMessage>,
    tx_err: Sender<MossdError>,

    gpu_vendor_info: GpuVendorInfo,
}

impl NvidiaInterface {
    async fn new(
        uuid: String,
        gpu_vendor_info: GpuVendorInfo,
        tx_dbus_service: Sender<StateManagerMessage>,
        tx_err: Sender<MossdError>,
    ) -> Result<Self> {
        Ok(Self {
            uuid,

            tx_dbus_service,
            tx_err,

            gpu_vendor_info,
        })
    }
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

impl DBusService {
    pub fn new() -> Self {
        Self {
            error_object_path: ERROR_OBJECT_PATH,
        }
    }

    pub async fn run(
        &mut self,
        run_token: CancellationToken,

        tx_dbus_to_manager: Sender<StateManagerMessage>,
        mut rx_manager_to_dbus: Receiver<DBusServiceMessage>,

        tx_err: Sender<MossdError>,
    ) {
        // Connect to the system D-Bus
        // TODO: Switch to system bus
        let connection =
            // TODO: Fix this ugly mess
            match Connection::session().await {
                Ok(conn) => conn,
                Err(err) => {
                    let channel_error = tx_err
                        .send(DbusServiceError::DBusConnection {
                            reason: format!(
                                "Failed to establish connection with the bus"
                            ),
                            error: err.into(),
                        }.into()).await;

                    if let Err(cerr) = channel_error {
                        error!("Failed to send error over channel: {cerr}")
                    }

                    // Just return, there is nothing else to do
                    return;
                }
            };

        trace!("DBus connection enstablished");

        if let Err(err) = self
            .initialize_service(&connection, tx_dbus_to_manager, tx_err.clone())
            .await
        {
            if let Err(cerr) = tx_err.send(err.into()).await {
                error!("Failed to send error over channel: {}", cerr);
            }
        }

        loop {
            select! {
                _ = run_token.cancelled() => {
                    info!("DBus service: Quiting");
                    break;
                },
                message = rx_manager_to_dbus.recv() => {
                    let res = self.parse_manager_message(
                        &connection,
                        message
                    ).await;

                    Self::send_error(res, tx_err.clone()).await;
                }
            }
        }
    }

    async fn parse_manager_message(
        &self,
        connection: &Connection,
        message: Option<DBusServiceMessage>,
    ) -> Result<()> {
        if message.is_none() {
            return Ok(());
        }

        match message.unwrap() {
            DBusServiceMessage::NewError(error) => {
                let code = error.error_code();
                let error_message = format!("{}", error);

                // Send a new error signal
                let intf_ref = Self::get_dbus_interface_ref::<ErrorInterface>(
                    &connection,
                    self.error_object_path,
                )
                .await?;

                intf_ref
                    .signal_emitter()
                    .new_error(code, &error_message)
                    .await
                    .map_err(|e| DbusServiceError::DBusSignal {
                        reason: format!("Failed to send NewError signal"),
                        error: e.into(),
                    })?;
            }
        }

        Ok(())
    }

    async fn initialize_service(
        &mut self,
        connection: &Connection,
        tx_dbus_to_manager: Sender<StateManagerMessage>,
        tx_err: Sender<MossdError>,
    ) -> Result<()> {
        // Initialize the error interface
        self.initialize_error_object(connection).await?;

        // Query the state manager to get a list of the available GPUs
        let (tx, rx) = oneshot::channel();
        let message = StateManagerMessage::GetGpus { tx };

        tx_dbus_to_manager.send(message).await.map_err(|_| {
            DbusServiceError::TX {
                reason: format!("Failed to send message to state manager"),
            }
        })?;

        // Wait for an answer
        let answer = rx.await.map_err(|e| DbusServiceError::RX {
            reason: format!("Error while waiting for state manager answer"),
            error: e.into(),
        })?;

        let gpu_uuids = if let StateManagerAnswer::Gpus(uuids) = answer {
            Ok(uuids)
        } else {
            Err(DbusServiceError::InvalidResponse {
                reason: format!("Invalid response from state manager"),
            })
        }?;

        // Create a D-Bus object for each GPUs on the system
        let mut gpu_count = 1;

        for uuid in gpu_uuids {
            trace!("Creating D-Bus object for GPU: {}", uuid);

            let path = format!("/com/github/Mossd1/Gpu{}", gpu_count);

            Self::initialize_gpu_object(
                path,
                uuid,
                connection,
                tx_dbus_to_manager.clone(),
                tx_err.clone(),
            )
            .await?;

            gpu_count += 1;
        }

        // Request the service name
        // NOTE:    The name request must happen AFTER setting up the
        //          server object or messages might be lost
        connection.request_name(SERVICE_NAME).await.map_err(|e| {
            DbusServiceError::DBusConnection {
                reason: format!("Failed to acquire service name"),
                error: e.into(),
            }
        })?;

        Ok(())
    }

    async fn initialize_error_object(
        &mut self,
        connection: &Connection,
    ) -> Result<()> {
        connection
            .object_server()
            .at(self.error_object_path, ErrorInterface::default())
            .await
            .map_err(|e| DbusServiceError::DBusObject {
                reason: format!("Error while initializing Error object"),
                error: e.into(),
            })?;

        Ok(())
    }

    async fn initialize_gpu_object(
        path: String,
        uuid: String,

        connection: &Connection,

        tx_dbus: Sender<StateManagerMessage>,
        tx_err: Sender<MossdError>,
    ) -> Result<()> {
        // Get the GPU vendor infos
        let (tx, rx) = oneshot::channel();
        let message = StateManagerMessage::GetGpuVendorInfo {
            uuid: uuid.clone(),
            tx,
        };

        tx_dbus
            .send(message)
            .await
            .map_err(|_| DbusServiceError::TX {
                reason: format!("Failed to send message to state manager"),
            })?;

        let answer = rx.await.map_err(|e| DbusServiceError::RX {
            reason: format!("Failed to receive answer from state manager"),
            error: e.into(),
        })?;

        let gpu_vendor_info =
            extract_answer!(StateManagerAnswer::GpuVendorInfo, answer)?;

        connection
            .object_server()
            .at(
                path.clone(),
                GpuInterface::new(
                    uuid.clone(),
                    tx_dbus.clone(),
                    tx_err.clone(),
                )
                .await?,
            )
            .await
            .map_err(|e| DbusServiceError::DBusObject {
                reason: format!("Error while initializing GPU object"),
                error: e.into(),
            })?;

        // Create a Nvidia interface if the GPU is Nvidia
        if matches!(gpu_vendor_info, GpuVendorInfo::Nvidia { .. }) {
            connection
                .object_server()
                .at(
                    path.clone(),
                    NvidiaInterface::new(
                        uuid.clone(),
                        gpu_vendor_info,
                        tx_dbus.clone(),
                        tx_err.clone(),
                    )
                    .await?,
                )
                .await
                .map_err(|e| DbusServiceError::DBusObject {
                    reason: format!("Error while initializing GPU object"),
                    error: e.into(),
                })?;
        }

        Ok(())
    }

    async fn get_dbus_interface_ref<T: Interface>(
        connection: &Connection,
        path: &str,
    ) -> Result<InterfaceRef<T>> {
        let obj_server = connection.object_server();

        obj_server.interface::<_, T>(path).await.map_err(|e| {
            DbusServiceError::DBusObject {
                reason: format!("Failed to get object interface"),
                error: e.into(),
            }
        })
    }

    async fn send_error<T>(
        res: Result<T>,
        tx_err: Sender<MossdError>,
    ) -> Option<T> {
        match res {
            Ok(data) => Some(data),
            Err(err) => {
                let channel_error = tx_err
                    .send(
                        DbusServiceError::DBusConnection {
                            reason: format!(
                                "Failed to establish connection with the bus"
                            ),
                            error: err.into(),
                        }
                        .into(),
                    )
                    .await;

                if let Err(cerr) = channel_error {
                    error!("Failed to send error over channel: {cerr}")
                }

                None
            }
        }
    }
}
