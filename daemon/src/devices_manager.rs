use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use nvml_wrapper::Nvml;
use tokio::{select, sync::mpsc::Sender};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::gpu_device::{GpuDevice, nvidia_device::NvidiaDevice};

pub struct DevicesManager {
    // Store NVML context for Nvidia GPUs
    // NVML is thread-safe so it is safe to make
    // simultaneous NVML calls from multiple threads.
    // We can therefore simply wrap it in a Arc with no Mutex
    nvml: Option<Arc<Nvml>>,

    devices: HashMap<String, Box<dyn GpuDevice + Send>>,
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

                    e.chain().for_each(|e| {
                        debug!("Error chain: {}", e);
                    });
                },
            );
        }

        Self { nvml, devices }
    }

    pub async fn run(
        &mut self,
        run_token: CancellationToken,
        //        mut rx_message: Receiver<GpusManagerMessage>,
        tx_err: Sender<anyhow::Error>,
    ) {
        loop {
            select! {
                _ = run_token.cancelled() => {
                    info!("GPUs manager: Quiting");

                    break;
                },
                //message = rx_message.recv() => {
                //    if let Err(err) = self.parse_message(message) {
                //        tx_err.send(err).await.unwrap_or_else(|err| {
                //            error!("Failed to send error over channel: {err}");
                //        });
                //    }
                //}
            }
        }
    }

    // Discover Nvidia GPUs on the system, create the associated
    // GPU devices and add them to the given hash map
    fn discover_nvidia_gpus(
        nvml: Arc<Nvml>,
        devices_map: &mut HashMap<String, Box<dyn GpuDevice + Send>>,
    ) -> Result<()> {
        let device_count = nvml.device_count()?;

        for i in 0..device_count {
            // Get the UUID of each device
            let device = nvml.device_by_index(i)?;
            let uuid = device.uuid()?;

            debug!("Found Nvidia device: \"{}\"", uuid);

            // Create the GPU device
            let device = Box::new(NvidiaDevice::new(nvml.clone(), &uuid)?);

            // Add the device to the hash map
            devices_map.insert(uuid, device);
        }

        Ok(())
    }
}
