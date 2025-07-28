use std::sync::Arc;

use anyhow::{Error, Result, anyhow};
use nvml_wrapper::Nvml;
use tokio::{
    select,
    sync::{
        mpsc::{Receiver, Sender},
        oneshot,
    },
};
use tokio_util::sync::CancellationToken;

use tracing::{error, trace};

use crate::{
    config_manager::{ConfigMessage, ConfigMessageAnswer},
    fan_curve::{
        FanCurve, hysteresis_curve::HysteresisCurve, linear_curve::LinearCurve,
    },
    fan_manager::{FanMessage, FanMode},
};

pub struct StateManager {
    nvml: Arc<Nvml>,

    tx_fan_manager: Sender<FanMessage>,
    tx_config_manager: Sender<ConfigMessage>,
}

impl StateManager {
    pub fn new(
        nvml: Arc<Nvml>,
        tx_fan_manager: Sender<FanMessage>,
        tx_config_manager: Sender<ConfigMessage>,
    ) -> Self {
        Self {
            nvml,

            tx_fan_manager,
            tx_config_manager,
        }
    }

    // Run the main state manager of the daemon
    pub async fn run(
        &mut self,
        run_token: CancellationToken,
        mut rx_err: Receiver<Error>,
    ) {
        // Find the UUIDs of the GPUs on the system
        let gpu_uuids = match self.find_gpus() {
            Ok(gpus) => gpus,
            Err(err) => {
                error!("{}", err);
                Vec::new()
            }
        };

        trace!("GPUs on current system: {:?}", gpu_uuids);

        // Load and apply the initial configuration
        self.apply_config(&gpu_uuids).await;

        loop {
            select! {
                _ = run_token.cancelled() => {
                    break;
                }
                err_message = rx_err.recv() => {
                    self.parse_error(err_message);
                }
            }
        }
    }

    // Query the configuration manager about the current settings
    // and applies them to the various system at start-up
    async fn apply_config(&mut self, uuids: &Vec<String>) {
        // Request and apply the configuration information for every GPUs
        for uuid in uuids {
            // Query the configuration manager
            let (tx, rx) = oneshot::channel();
            let message = ConfigMessage::GetGpu {
                uuid: uuid.clone(),
                tx: tx,
            };

            if let Err(err) = self.tx_config_manager.send(message).await {
                error!("{}", err);

                continue;
            }

            // Wait for an answer
            if let Ok(answer) = rx.await {
                if let ConfigMessageAnswer::Gpu(gpu) = answer {
                    let profile = gpu.profile;

                    if let Err(err) = self.apply_profile(&uuid, &profile).await
                    {
                        error!("{}", err);
                        continue;
                    }
                } else {
                    error!("Wrong answer recieved from config manager");
                }
            }
        }
    }

    async fn apply_profile(
        &mut self,
        uuid: &str,
        profile_name: &str,
    ) -> Result<()> {
        // Query the configuration manager
        let (tx, rx) = oneshot::channel();
        let message = ConfigMessage::GetProfile {
            name: profile_name.to_string(),
            tx: tx,
        };

        self.tx_config_manager.send(message).await?;

        // Wait for an answer
        let answer = rx.await?;

        if let ConfigMessageAnswer::Profile(profile) = answer {
            // Applies the profile's fan curve to the GPU
            let fan_curve = &profile.fan_curve;
            self.apply_fan_config(uuid, fan_curve).await?;

            // TODO: Apply overclock and power limit settings
        }

        Ok(())
    }

    // Apply the fan curve config in the
    async fn apply_fan_config(
        &mut self,
        uuid: &str,
        curve_name: &str,
    ) -> Result<()> {
        // Query the configuration manager
        let (tx, rx) = oneshot::channel();
        let message = ConfigMessage::GetFanCurve {
            name: curve_name.to_string(),
            tx: tx,
        };

        self.tx_config_manager.send(message).await?;

        // Wait for an answer
        let answer = rx.await?;

        if let ConfigMessageAnswer::FanCurve(curve) = answer {
            let mut new_curve = Box::new(HysteresisCurve::new(
                LinearCurve::new(),
                curve.hysteresis_down,
                curve.hysteresis_up,
            ));

            for point in curve.points {
                new_curve.add_point(point.into());
            }

            // Send the curve to the fan manager
            let message = FanMessage::UpdateCurve {
                uuid: uuid.to_string(),
                new_curve,
            };

            self.tx_fan_manager
                .send(message)
                .await
                .map_err(|err| anyhow!("{err}"))?;

            // Set the fan mode according to the configuration
            let message = if curve.manual {
                FanMessage::SetMode {
                    uuid: uuid.to_string(),
                    mode: FanMode::Manual,
                }
            } else {
                FanMessage::SetMode {
                    uuid: uuid.to_string(),
                    mode: FanMode::Auto,
                }
            };

            self.tx_fan_manager
                .send(message)
                .await
                .map_err(|err| anyhow!("{err}"))?;

            Ok(())
        } else {
            Err(anyhow!("Wrong answer recieved from config manager"))
        }
    }

    // Return a vector of string representing the UUIDs
    // of all of the GPUs on the system
    fn find_gpus(&mut self) -> Result<Vec<String>> {
        let gpu_count = self.nvml.device_count()?;
        let mut gpu_uuids = Vec::with_capacity(gpu_count as usize);

        for i in 0..gpu_count {
            let device = self.nvml.device_by_index(i)?;
            let uuid = device.uuid()?;

            gpu_uuids.push(uuid);
        }

        Ok(gpu_uuids)
    }

    // Parse and log an error message
    fn parse_error(&mut self, err_message: Option<Error>) {
        // Log the full error chain for each error
        if let Some(err_chain) = err_message {
            for err in err_chain.chain() {
                error!("{}", err);
            }
        }
    }
}
