use std::time::Duration;

use thiserror::Error;
use tokio::{
    select,
    sync::{
        mpsc::{Receiver, Sender},
        oneshot,
    },
};
use tokio_util::sync::CancellationToken;

use tracing::error;

use crate::{
    config_manager::{ConfigMessage, ConfigMessageAnswer},
    dbus_service::DBusServiceMessage,
    devices_manager::{DevicesManagerAnswer, DevicesManagerMessage},
    errors::MossdError,
    fan_curve::{
        fan_curve_info::FanCurveInfo, fan_mode::FanMode,
        hysteresis_curve::HysteresisCurve, linear_curve::LinearCurve,
    },
    gpu_device::{
        gpu_config::GpuConfig,
        gpu_info::{GpuInfo, GpuVendorInfo},
    },
};

macro_rules! extract_answer {
    ( $expected:path, $answer:expr ) => {{
        let result = if let $expected(data) = $answer {
            Ok(data)
        } else {
            Err(StateManagerError::InvalidResponse {
                reason: format!("Invalid responce {:?}", $answer),
            })
        };

        result
    }};
}

type Result<T> = std::result::Result<T, StateManagerError>;

type Responder = oneshot::Sender<StateManagerAnswer>;

#[derive(Debug, Error)]
pub enum StateManagerError {
    #[error("State manager TX error: {reason}")]
    TX { reason: String },
    #[error("State manager RX error: {reason}")]
    RX {
        reason: String,
        error: anyhow::Error,
    },
    #[error("State manager invalid response error: {reason}")]
    InvalidResponse { reason: String },
}

// This is the message enum that the D-Bus service process will
// send to the state manger to request data or set properties
pub enum StateManagerMessage {
    // Get the UUIDs of all the GPUs on the system
    GetGpus { tx: Responder },

    // Get the GPU infos
    GetGpuInfo { uuid: String, tx: Responder },
    GetGpuVendorInfo { uuid: String, tx: Responder },
}

// This is the answer enum that the state manager will use to
// communicate with the D-Bus service
#[derive(Debug)]
pub enum StateManagerAnswer {
    Gpus(Vec<String>),

    GpuInfo(GpuInfo),
    GpuVendorInfo(GpuVendorInfo),
}

pub struct StateManager {
    tx_config_manager: Sender<ConfigMessage>,
    tx_devices_manager: Sender<DevicesManagerMessage>,

    // D-Bus service channels
    rx_dbus_to_manager: Receiver<StateManagerMessage>,
    tx_manager_to_dbus: Sender<DBusServiceMessage>,
}

impl StateManager {
    pub fn new(
        tx_config_manager: Sender<ConfigMessage>,
        tx_devices_manager: Sender<DevicesManagerMessage>,

        rx_dbus_to_manager: Receiver<StateManagerMessage>,
        tx_manager_to_dbus: Sender<DBusServiceMessage>,
    ) -> Self {
        Self {
            tx_config_manager,
            tx_devices_manager,
            rx_dbus_to_manager,
            tx_manager_to_dbus,
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
            self.parse_error(Some(e.into())).await;
        }

        loop {
            select! {
                _ = run_token.cancelled() => {
                    break;
                }
                err_message = rx_err.recv() => {
                    self.parse_error(err_message).await;
                }
                message = self.rx_dbus_to_manager.recv() => {
                    if let Err(e) = self.parse_state_message(message).await {
                        self.parse_error(Some(e.into())).await;
                    }
                }
            }
        }
    }

    // Send a query to the device manager
    async fn query_device_manager(
        &mut self,
        message: DevicesManagerMessage,
        rx: oneshot::Receiver<DevicesManagerAnswer>,
    ) -> Result<DevicesManagerAnswer> {
        self.tx_devices_manager.send(message).await.map_err(|_| {
            StateManagerError::TX {
                reason: format!("Failed to send request to devices manager"),
            }
        })?;

        let answer = rx.await.map_err(|e| StateManagerError::RX {
            reason: format!("Failed to receive answer form devices manager"),
            error: e.into(),
        })?;

        Ok(answer)
    }

    // Send a query to the config manager
    async fn query_config_manager(
        &mut self,
        message: ConfigMessage,
        rx: oneshot::Receiver<ConfigMessageAnswer>,
    ) -> Result<ConfigMessageAnswer> {
        self.tx_config_manager.send(message).await.map_err(|_| {
            StateManagerError::TX {
                reason: format!("Failed to send request to config manager"),
            }
        })?;

        let answer = rx.await.map_err(|e| StateManagerError::RX {
            reason: format!("Failed to receive answer form config manager"),
            error: e.into(),
        })?;

        Ok(answer)
    }

    async fn parse_state_message(
        &mut self,
        message: Option<StateManagerMessage>,
    ) -> Result<()> {
        if let Some(message) = message {
            let answer = match message {
                StateManagerMessage::GetGpus { tx: tx_answer } => {
                    // Request the device list to the device manager
                    let (tx, rx) = oneshot::channel();
                    let message = DevicesManagerMessage::ListDevices { tx };
                    let answer = self.query_device_manager(message, rx).await?;

                    let uuids = extract_answer!(
                        DevicesManagerAnswer::DeviceList,
                        answer
                    )?;

                    Some((tx_answer, StateManagerAnswer::Gpus(uuids)))
                }
                StateManagerMessage::GetGpuInfo {
                    uuid,
                    tx: tx_answer,
                } => {
                    let (tx, rx) = oneshot::channel();
                    let message =
                        DevicesManagerMessage::GetDeviceInfo { uuid, tx };
                    let answer = self.query_device_manager(message, rx).await?;

                    let device_info = extract_answer!(
                        DevicesManagerAnswer::DeviceInfo,
                        answer
                    )?;

                    Some((tx_answer, StateManagerAnswer::GpuInfo(device_info)))
                }
                StateManagerMessage::GetGpuVendorInfo {
                    uuid,
                    tx: tx_answer,
                } => {
                    let (tx, rx) = oneshot::channel();
                    let message =
                        DevicesManagerMessage::GetDeviceVendorInfo { uuid, tx };
                    let answer = self.query_device_manager(message, rx).await?;

                    let device_vendor_info = extract_answer!(
                        DevicesManagerAnswer::DeviceVendorInfo,
                        answer
                    )?;

                    Some((
                        tx_answer,
                        StateManagerAnswer::GpuVendorInfo(device_vendor_info),
                    ))
                }
            };

            // Send the message to channel if needed
            if let Some((tx, answer)) = answer {
                if let Err(err) = tx.send(answer) {
                    error!("{:?}", err);
                }
            }
        }

        Ok(())
    }

    // Query the configuration manager about the current settings
    // and applies them to the various devices at start-up
    async fn apply_settings(&mut self) -> Result<()> {
        // Get the UUIDs of the devices on the system
        let (answer_tx, answer_rx) = oneshot::channel();

        let answer = self
            .query_device_manager(
                DevicesManagerMessage::ListDevices { tx: answer_tx },
                answer_rx,
            )
            .await?;

        let uuids = extract_answer!(DevicesManagerAnswer::DeviceList, answer)?;

        // Request and apply the configuration information for every GPUs
        for uuid in uuids {
            // Query the configuration manager for the fan curve
            let (tx, rx) = oneshot::channel();
            let message = ConfigMessage::GetFanCurve {
                uuid: uuid.clone(),
                tx,
            };

            let answer = self.query_config_manager(message, rx).await?;
            let fan_curve_info =
                extract_answer!(ConfigMessageAnswer::FanCurve, answer)?;

            // Apply the fan curve settings
            self.apply_fan_curve(&uuid, fan_curve_info).await?;

            // Query the configuration manager for the fan update interval
            let (tx, rx) = oneshot::channel();
            let message = ConfigMessage::GetFanUpdateInterval {
                uuid: uuid.clone(),
                tx,
            };

            let answer = self.query_config_manager(message, rx).await?;
            let update_interval = extract_answer!(
                ConfigMessageAnswer::FanUpdateInterval,
                answer
            )?;

            // Apply the fan curve settings
            self.apply_fan_update_interval(&uuid, update_interval)
                .await?;

            // Query the configuration manager for the fan mode
            let (tx, rx) = oneshot::channel();
            let message = ConfigMessage::GetFanMode {
                uuid: uuid.clone(),
                tx,
            };

            let answer = self.query_config_manager(message, rx).await?;
            let fan_mode =
                extract_answer!(ConfigMessageAnswer::FanMode, answer)?;

            // Apply the fan mode
            self.apply_fan_mode(&uuid, fan_mode).await?;

            // Query the configuration manager for the fan update interval
            let (tx, rx) = oneshot::channel();
            let message = ConfigMessage::GetConfig {
                uuid: uuid.clone(),
                tx,
            };

            let answer = self.query_config_manager(message, rx).await?;
            let config = extract_answer!(ConfigMessageAnswer::Config, answer)?;

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

        self.tx_devices_manager.send(message).await.map_err(|_| {
            StateManagerError::TX {
                reason: format!("Failed to send request to devices manager"),
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

            self.tx_devices_manager.send(message).await.map_err(|_| {
                StateManagerError::TX {
                    reason: format!(
                        "Failed to send request to devices manager"
                    ),
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

            self.tx_devices_manager.send(message).await.map_err(|_| {
                StateManagerError::TX {
                    reason: format!(
                        "Failed to send request to devices manager"
                    ),
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

            self.tx_devices_manager.send(message).await.map_err(|_| {
                StateManagerError::TX {
                    reason: format!(
                        "Failed to send request to devices manager"
                    ),
                }
            })?;
        }

        Ok(())
    }

    // Parse and log an error message
    async fn parse_error(&mut self, err_message: Option<MossdError>) {
        if err_message.is_none() {
            return;
        }

        // Log the full error chain for each error
        error!("{}", err_message.as_ref().unwrap());

        // Send the error to the D-Bus service
        let message = DBusServiceMessage::NewError(err_message.unwrap());

        if let Err(_) = self.tx_manager_to_dbus.send(message).await {
            error!("Failed to send error message to D-Bus service");
        }
    }
}
