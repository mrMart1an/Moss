use anyhow::anyhow;
use tokio::{
    select,
    sync::{mpsc::Sender, oneshot},
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, trace};
use zbus::{Connection, interface};

const SERVICE_NAME: &str = "com.github.Mossd1";

type Responder = oneshot::Sender<DBusServiceAnswer>;

// This is the message enum that the D-Bus service process will
// send to the state manger to request data or set properties
pub enum DBusServiceMessage {
    GetGpusUuid(Responder),

    GetName { uuid: String, tx: Responder },

    GetDriverVersion { uuid: String, tx: Responder },
    GetVbios { uuid: String, tx: Responder },
    GetNumCores { uuid: String, tx: Responder },
    GetPcieWidth { uuid: String, tx: Responder },
    GetPcieGen { uuid: String, tx: Responder },

    GetTemp { uuid: String, tx: Responder },

    GetGraphicsFreq { uuid: String, tx: Responder },
    GetVideoFreq { uuid: String, tx: Responder },
    GetSmFreq { uuid: String, tx: Responder },
    GetMemFreq { uuid: String, tx: Responder },

    GetGraphicsBoostFreq { uuid: String, tx: Responder },
    GetVideoBoostFreq { uuid: String, tx: Responder },
    GetSmBoostFreq { uuid: String, tx: Responder },
    GetMemBoostFreq { uuid: String, tx: Responder },

    GetCoreClockOffset { uuid: String, tx: Responder },
    GetMemClockOffset { uuid: String, tx: Responder },

    GetPowerUsage { uuid: String, tx: Responder },
    GetPowerLimit { uuid: String, tx: Responder },
    GetPowerLimitMax { uuid: String, tx: Responder },
    GetPowerLimitMin { uuid: String, tx: Responder },
    GetPowerDefault { uuid: String, tx: Responder },

    GetFanSpeed { uuid: String, tx: Responder },
    GetFanRpm { uuid: String, tx: Responder },

    GetCoreUsage { uuid: String, tx: Responder },
    GetMemUsage { uuid: String, tx: Responder },

    GetTotalMemory { uuid: String, tx: Responder },
    GetUsedMemory { uuid: String, tx: Responder },
    GetFreeMemory { uuid: String, tx: Responder },

    GetMaxTemp { uuid: String, tx: Responder },
    GetMemMaxTemp { uuid: String, tx: Responder },
    GetSlowdownTemp { uuid: String, tx: Responder },
    GetShutdownTemp { uuid: String, tx: Responder },

    // Setters
    SetCoreClockOffset { uuid: String, offset: i32 },
    SetMemClockOffset { uuid: String, offset: i32 },

    SetPowerLimit { uuid: String, limit: u32 },
}

// This is the answer enum that the state manager will use to
// communicate with the D-Bus service
#[derive(Debug)]
pub enum DBusServiceAnswer {
    GpusUuid(Vec<String>),

    Name(String),

    DriverVersion(String),
    Vbios(String),
    NumCores(Option<u32>),
    PcieWidth(Option<u32>),
    PcieGen(Option<u32>),

    Temp(Option<u32>),

    GraphicsFreq(Option<u32>),
    VideoFreq(Option<u32>),
    SmFreq(Option<u32>),
    MemFreq(Option<u32>),

    GraphicsBoostFreq(Option<u32>),
    VideoBoostFreq(Option<u32>),
    SmBoostFreq(Option<u32>),
    MemBoostFreq(Option<u32>),

    CoreClockOffset(Option<i32>),
    MemClockOffset(Option<i32>),

    PowerUsage(Option<u32>),
    PowerLimit(Option<u32>),
    PowerLimitMax(u32),
    PowerLimitMin(u32),
    PowerDefault(u32),

    FanSpeed(Option<u32>),
    FanRpm(Option<u32>),

    CoreUsage(Option<u32>),
    MemUsage(Option<u32>),

    TotalMemory(Option<u64>),
    UsedMemory(Option<u64>),
    FreeMemory(Option<u64>),

    MaxTemp(Option<u32>),
    MemMaxTemp(Option<u32>),
    SlowdownTemp(Option<u32>),
    ShutdownTemp(Option<u32>),
}

pub struct DBusService;

// GPU D-Bus interface
struct GpuInterface {
    uuid: String,
    tx_dbus_service: Sender<DBusServiceMessage>,
}

impl GpuInterface {
    fn new(uuid: String, tx_dbus_service: Sender<DBusServiceMessage>) -> Self {
        Self { uuid, tx_dbus_service }
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
        Self { }
    }

    pub async fn run(
        &mut self,
        run_token: CancellationToken,
        tx_dbus_service: Sender<DBusServiceMessage>,
        tx_err: Sender<anyhow::Error>,
    ) {
        // Connect to the system D-Bus
        // TODO: Switch to system bus
        let connection = match Connection::session().await {
            Ok(con) => con,
            Err(err) => {
                if let Err(err) = tx_err.send(err.into()).await {
                    error!("Failed to send error over channel: {err}");
                }

                return;
            }
        };

        // Query the state manager to get a list of the available GPUs
        let (tx, rx) = oneshot::channel();
        let message = DBusServiceMessage::GetGpusUuid(tx);

        if let Err(err) = tx_dbus_service.send(message).await {
            tx_err.send(err.into()).await.unwrap_or_else(|err| {
                error!("Failed to send error over channel: {err}");
            });

            return;
        }

        // Wait for an answer
        let gpu_uuids = if let Ok(answer) = rx.await {
            if let DBusServiceAnswer::GpusUuid(uuids) = answer {
                uuids
            } else {
                tx_err
                    .send(anyhow!("Received wrong reply from state manager"))
                    .await
                    .unwrap_or_else(|err| {
                        error!("Failed to send error over channel: {err}");
                    });

                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Create a D-Bus object for each GPUs on the system
        let mut gpu_count = 1;

        for uuid in gpu_uuids {
            trace!("Creating D-Bus object for GPU: {}", uuid);

            let path = format!("/com/github/Mossd1/Gpu{}", gpu_count);
            let tx = tx_dbus_service.clone();

            if let Err(err) = connection
                .object_server()
                .at(path, GpuInterface::new(uuid.clone(), tx))
                .await
            {
                tx_err.send(err.into()).await.unwrap_or_else(|err| {
                    error!("Failed to send error over channel: {err}");
                });

                continue;
            }

            gpu_count += 1;
        }

        // Request the service name
        // NOTE:    The name request must happen AFTER setting up the
        //          server object or messages might be lost
        if let Err(err) = connection.request_name(SERVICE_NAME).await {
            if let Err(err) = tx_err.send(err.into()).await {
                error!("Failed to send error over channel: {err}");
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
}
