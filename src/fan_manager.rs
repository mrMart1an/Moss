use std::{sync::{Arc, Mutex}, time::Duration};

use anyhow::Result;
use nvml_wrapper::Nvml;
use tokio::{select, sync::mpsc::Receiver};
use tokio_util::sync::CancellationToken;

use crate::fan_curve::FanCurve;

pub enum FanMessage {
    Auto,
    Manual,
    UpdateCurve { new_curve: Box<dyn FanCurve + Send> },
    UpdateInterval { new_duration: Duration },
}

pub struct FanManager {
    nvml: Arc<Nvml>,

    automatic: bool,
    curve: Option<Box<dyn FanCurve + Send>>,

    // Update interval in seconds
    update_interval: Duration,
}

impl FanManager {
    pub fn new(nvml: Arc<Nvml>) -> Self {
        Self {
            nvml,

            automatic: false,
            curve: None,

            update_interval: Duration::from_secs_f32(2.),
        }
    }

    // Run the fan manager
    pub async fn run(
        &mut self,
        run_token: CancellationToken,
        mut rx_channel: Receiver<FanMessage>,
    ) -> Result<()> {

        loop {
            select! {
                _ = run_token.cancelled() => {
                    println!("Fan manager: Quiting");
                    break;
                },
                _msg = rx_channel.recv() => {

                },
                _ = tokio::time::sleep(self.update_interval) => {
                    println!("Fan manager: Updating");
                }
            }
        }

        Ok(())
    }
}

