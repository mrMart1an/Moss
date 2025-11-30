use std::time::Duration;

use anyhow::anyhow;
use thiserror::Error;
use tokio::{
    select,
    sync::{
        mpsc::{Receiver, Sender},
        oneshot,
    },
};
use tokio_util::sync::CancellationToken;

use tracing::{error, warn};

use crate::{
    config_manager::{ConfigMessage, ConfigMessageAnswer},
    devices_manager::{DevicesManagerAnswer, DevicesManagerMessage},
    errors::MossdError,
    fan_curve::{
        fan_curve_info::FanCurveInfo,
        fan_mode::FanMode,
        hysteresis_curve::HysteresisCurve,
        linear_curve::LinearCurve,
    },
    gpu_device::gpu_config::GpuConfig,
};

type Result<T> = std::result::Result<T, StateManagerError>;

#[derive(Debug, Error)]
pub enum StateManagerError {
    #[error("State manager TX error: {reason}")]
    TX {
        reason: String,
        error: anyhow::Error,
    },
    #[error("State manager RX error: {reason}")]
    RX {
        reason: String,
        error: anyhow::Error,
    },
    #[error("State manager invalid response error: {reason}")]
    InvalidResponse { reason: String },
}

pub struct StateManager {
    tx_config_manager: Sender<ConfigMessage>,
    tx_devices_manager: Sender<DevicesManagerMessage>,
}

impl StateManager {
    pub fn new(
        tx_config_manager: Sender<ConfigMessage>,
        tx_devices_manager: Sender<DevicesManagerMessage>,
    ) -> Self {
        Self {
            tx_config_manager,
            tx_devices_manager,
        }
    }

    // Run the main state manager of the daemon
    pub async fn run(
        &mut self,
        run_token: CancellationToken,
        mut rx_err: Receiver<MossdError>,
    ) {
        // Load and apply the initial configuration
        if let Err(e) = self.apply_settings().await {
            self.parse_error(Some(e.into()));
        }

        loop {
            select! {
                _ = run_token.cancelled() => {
                    break;
                }
                err_message = rx_err.recv() => {
                    self.parse_error(err_message);
                }
                //message = self.rx_dbus_service.recv() => {
                //    self.parse_dbus_message(message).await;
                //}
            }
        }
    }

    //async fn parse_dbus_message(
    //    &mut self,
    //    message: Option<DBusServiceMessage>,
    //) {
    //    if let Some(message) = message {
    //        let (tx, answer) = match message {
    //            DBusServiceMessage::GetGpusUuid(tx) => {
    //                let uuids = self.gpu_uuids.clone();
    //                (Some(tx), Some(DBusServiceAnswer::GpusUuid(uuids)))
    //            }
    //            _ => { (None, None) }
    //        };

    //        // Send the message to channel if needed
    //        if let (Some(tx), Some(answer)) = (tx, answer) {
    //            if let Err(err) = tx.send(answer) {
    //                error!("{:?}", err);
    //            }
    //        }
    //    }
    //}

    // Query the configuration manager about the current settings
    // and applies them to the various devices at start-up
    async fn apply_settings(&mut self) -> Result<()> {
        // Get the UUIDs of the devices on the system
        let (answer_tx, answer_rx) = oneshot::channel();

        self.tx_devices_manager
            .send(DevicesManagerMessage::ListDevices { tx: answer_tx })
            .await
            .map_err(|e| StateManagerError::TX {
                reason: format!("Failed to send request to devices manager"),
                error: anyhow!("{}", e),
            })?;

        let answer = answer_rx.await.map_err(|e| StateManagerError::RX {
            reason: format!("Failed to receive answer form devices manager"),
            error: e.into(),
        })?;

        let uuids = if let DevicesManagerAnswer::DeviceList(uuids_list) = answer
        {
            uuids_list
        } else {
            return Err(StateManagerError::InvalidResponse {
                reason: format!("Invalid responce from devices manager"),
            });
        };

        // Request and apply the configuration information for every GPUs
        for uuid in uuids {
            // Query the configuration manager for the fan curve
            let (tx, rx) = oneshot::channel();
            let message = ConfigMessage::GetFanCurve {
                uuid: uuid.clone(),
                tx,
            };

            self.tx_config_manager.send(message).await.map_err(|e| {
                StateManagerError::TX {
                    reason: format!("Failed to send query to config manager"),
                    error: anyhow!("{}", e),
                }
            })?;

            let answer = rx.await.map_err(|e| StateManagerError::RX {
                reason: format!("Failed to receive answer form config manager"),
                error: e.into(),
            })?;

            let fan_curve_info =
                if let ConfigMessageAnswer::FanCurve(data) = answer {
                    data
                } else {
                    return Err(StateManagerError::InvalidResponse {
                        reason: format!("Invalid responce from config manager"),
                    });
                };

            // Apply the fan curve settings
            self.apply_fan_curve(&uuid, fan_curve_info).await?;

            // Query the configuration manager for the fan update interval
            let (tx, rx) = oneshot::channel();
            let message = ConfigMessage::GetFanUpdateInterval {
                uuid: uuid.clone(),
                tx,
            };

            self.tx_config_manager.send(message).await.map_err(|e| {
                StateManagerError::TX {
                    reason: format!("Failed to send query to config manager"),
                    error: anyhow!("{}", e),
                }
            })?;

            let answer = rx.await.map_err(|e| StateManagerError::RX {
                reason: format!("Failed to receive answer form config manager"),
                error: e.into(),
            })?;

            let update_interval =
                if let ConfigMessageAnswer::FanUpdateInterval(data) = answer {
                    data
                } else {
                    return Err(StateManagerError::InvalidResponse {
                        reason: format!("Invalid responce from config manager"),
                    });
                };

            // Apply the fan curve settings
            self.apply_fan_update_interval(&uuid, update_interval)
                .await?;

            // Query the configuration manager for the fan mode
            let (tx, rx) = oneshot::channel();
            let message = ConfigMessage::GetFanMode {
                uuid: uuid.clone(),
                tx,
            };

            self.tx_config_manager.send(message).await.map_err(|e| {
                StateManagerError::TX {
                    reason: format!("Failed to send query to config manager"),
                    error: anyhow!("{}", e),
                }
            })?;

            let answer = rx.await.map_err(|e| StateManagerError::RX {
                reason: format!("Failed to receive answer form config manager"),
                error: e.into(),
            })?;

            let fan_mode = if let ConfigMessageAnswer::FanMode(data) = answer {
                data
            } else {
                return Err(StateManagerError::InvalidResponse {
                    reason: format!("Invalid responce from config manager"),
                });
            };

            // Apply the fan mode
            self.apply_fan_mode(&uuid, fan_mode).await?;

            // Query the configuration manager for the fan update interval
            let (tx, rx) = oneshot::channel();
            let message = ConfigMessage::GetConfig {
                uuid: uuid.clone(),
                tx,
            };

            self.tx_config_manager.send(message).await.map_err(|e| {
                StateManagerError::TX {
                    reason: format!("Failed to send query to config manager"),
                    error: anyhow!("{}", e),
                }
            })?;

            let answer = rx.await.map_err(|e| StateManagerError::RX {
                reason: format!("Failed to receive answer form config manager"),
                error: e.into(),
            })?;

            let config = if let ConfigMessageAnswer::Config(data) = answer {
                data
            } else {
                return Err(StateManagerError::InvalidResponse {
                    reason: format!("Invalid responce from config manager"),
                });
            };

            // Apply the fan curve settings
            self.apply_config(&uuid, config).await?;
        }

        Ok(())
    }

    async fn apply_fan_mode(
        &mut self,
        uuid: &str,
        fan_mode: FanMode,
    ) -> Result<()> {
        let message = DevicesManagerMessage::SetDeviceFanMode {
            uuid: uuid.to_string(),
            fan_mode,
        };

        self.tx_devices_manager.send(message).await.map_err(|e| {
            StateManagerError::TX {
                reason: format!("Failed to send request to devices manager"),
                error: anyhow!("{}", e),
            }
        })?;

        Ok(())
    }

    // Apply the fan curve to the device
    async fn apply_fan_curve(
        &mut self,
        uuid: &str,
        curve_info_opt: Option<FanCurveInfo>,
    ) -> Result<()> {
        // Only apply fan curve settings if the config manager
        // returned fan curve info
        if let Some(fan_curve_info) = curve_info_opt {
            // Generate the actual fan curve to
            // then pass to the devices manager
            let fan_curve = Box::new(
                HysteresisCurve::<LinearCurve>::from_info(&fan_curve_info),
            );

            let message = DevicesManagerMessage::SetDeviceFanCurve {
                uuid: uuid.to_string(),
                fan_curve,
            };

            self.tx_devices_manager.send(message).await.map_err(|e| {
                StateManagerError::TX {
                    reason: format!(
                        "Failed to send request to devices manager"
                    ),
                    error: anyhow!("{}", e),
                }
            })?;
        }

        Ok(())
    }

    async fn apply_fan_update_interval(
        &mut self,
        uuid: &str,
        update_interval_opt: Option<Duration>,
    ) -> Result<()> {
        // Only apply fan update interval settings if the config manager
        // returned a duration value
        if let Some(interval) = update_interval_opt {
            let message = DevicesManagerMessage::SetDeviceFanUpdateInterval {
                uuid: uuid.to_string(),
                interval,
            };

            self.tx_devices_manager.send(message).await.map_err(|e| {
                StateManagerError::TX {
                    reason: format!(
                        "Failed to send request to devices manager"
                    ),
                    error: anyhow!("{}", e),
                }
            })?;
        }

        Ok(())
    }

    async fn apply_config(
        &mut self,
        uuid: &str,
        config_opt: Option<GpuConfig>,
    ) -> Result<()> {
        // Only apply config settings if the config manager
        // returned a config profile
        if let Some(config) = config_opt {
            let message = DevicesManagerMessage::ApplyDeviceGpuConfig {
                uuid: uuid.to_string(),
                config,
            };

            self.tx_devices_manager.send(message).await.map_err(|e| {
                StateManagerError::TX {
                    reason: format!(
                        "Failed to send request to devices manager"
                    ),
                    error: anyhow!("{}", e),
                }
            })?;
        }

        Ok(())
    }

    // Parse and log an error message
    fn parse_error(&mut self, err_message: Option<MossdError>) {
        // Log the full error chain for each error
        if let Some(err) = err_message {
            error!("{}", err);
        } else {
            warn!("Parsing empty error message");
        }
    }
}
