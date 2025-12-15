use std::fmt::format;

use anyhow::anyhow;
use thiserror::Error;
use tokio::{
    select,
    sync::{mpsc::Sender, oneshot},
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, trace};
use zbus::{Connection, interface};

use crate::errors::MossdError;

const SERVICE_NAME: &str = "com.github.Mossd1";

type Responder = oneshot::Sender<DBusServiceAnswer>;

type Result<T> = std::result::Result<T, DbusServiceError>;

#[derive(Debug, Error)]
pub enum DbusServiceError {
    #[error("DBus service manager TX error: {reason}")]
    TX {
        reason: String,
        error: anyhow::Error,
    },
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
}

// This is the message enum that the D-Bus service process will
// send to the state manger to request data or set properties
pub enum DBusServiceMessage {
    // Get the UUIDs of all the GPUs on the system
    GetGpus { tx: Responder },
}

// This is the answer enum that the state manager will use to
// communicate with the D-Bus service
#[derive(Debug)]
pub enum DBusServiceAnswer {
    Gpus { uuids: Vec<String> },
}

pub struct DBusService;

// GPU D-Bus interface
struct GpuInterface {
    uuid: String,
    tx_dbus_service: Sender<DBusServiceMessage>,
}

impl GpuInterface {
    fn new(uuid: String, tx_dbus_service: Sender<DBusServiceMessage>) -> Self {
        Self {
            uuid,
            tx_dbus_service,
        }
    }
}

#[interface(name = "com.github.Mossd1.Gpu")]
impl GpuInterface {
    #[zbus(property)]
    async fn temp(&self) -> u32 {
        32
    }
}

impl DBusService {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn run(
        &mut self,
        run_token: CancellationToken,
        tx_dbus_service: Sender<DBusServiceMessage>,
        tx_err: Sender<MossdError>,
    ) {
        // Connect to the system D-Bus
        // TODO: Switch to system bus
        let connection =
            // TODO: Fix this ugly mess
            match Connection::session().await {
                Ok(conn) => conn,
                Err(err) => {
                    if let Err(cerr) = tx_err
                        .send(DbusServiceError::DBusConnection {
                            reason: format!(
                                "Failed to establish connection with the bus"
                            ),
                            error: err.into(),
                        }.into()).await
                {
                    error!("Failed to send error over channel: {cerr}")
                }

                    // Just return, there is nothing else to do
                    return;
                }
            };

        trace!("DBus connection enstablished");

        if let Err(err) =
            self.initialize_service(&connection, tx_dbus_service).await
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
                }
            }
        }
    }

    async fn initialize_service(
        &mut self,
        connection: &Connection,
        tx_dbus_service: Sender<DBusServiceMessage>,
    ) -> Result<()> {
        // Query the state manager to get a list of the available GPUs
        let (tx, rx) = oneshot::channel();
        let message = DBusServiceMessage::GetGpus { tx };

        tx_dbus_service.send(message).await.map_err(|e| {
            DbusServiceError::TX {
                reason: format!("Failed to send message to state manager"),
                error: anyhow!("{:?}", e),
            }
        })?;

        // Wait for an answer
        let answer = rx.await.map_err(|e| DbusServiceError::RX {
            reason: format!("Error while waiting for state manager answer"),
            error: e.into(),
        })?;

        let gpu_uuids = if let DBusServiceAnswer::Gpus { uuids } = answer {
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
            let tx = tx_dbus_service.clone();

            connection
                .object_server()
                .at(path, GpuInterface::new(uuid.clone(), tx))
                .await
                .map_err(|e| DbusServiceError::DBusObject {
                    reason: format!("Error while initializing GPU object"),
                    error: e.into(),
                })?;

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
}
