use std::sync::Arc;

use anyhow::{Context, Result};
use moss_nv::{
    fan_manager::FanManager,
    logger,
};
use nvml_wrapper::Nvml;
use tokio::{signal::ctrl_c, sync::mpsc};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

#[tokio::main]
async fn main() -> Result<()> {
    logger::init_logging();

    // This token and tracker will be used to handle graceful shutdown
    let tracker = TaskTracker::new();
    let token = CancellationToken::new();

    // NVML is thread-safe so it is safe to make
    // simultaneous NVML calls from multiple threads.
    // We can therefore simply wrap it in a Arc with no Mutex
    let nvml =
        Arc::new(Nvml::init().with_context(|| "Failed to load NVML library")?);

    // Start the fan speed manager
    let (_tx_fan_manager, rx_fan_manager) = mpsc::channel(16);
    {
        let nvml = nvml.clone();
        let token = token.clone();

        tracker.spawn(async move {
            let mut fan_manager = FanManager::new(nvml);
            fan_manager.run(token, rx_fan_manager).await.unwrap();
        });
    }

    // TODO: Handle different unix signal for graceful termination
    ctrl_c().await?;

    // Cancel the token to communicate the program
    // termination to the running tasks
    token.cancel();

    // Wait for the tasks to finish
    tracker.close();
    tracker.wait().await;

    Ok(())
}
