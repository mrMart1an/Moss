use std::sync::Arc;

use anyhow::{Context, Result};
use mossd::{
    arg_parser::ArgsOptions,
    config_manager::ConfigManager,
    fan_manager::FanManager,
    logger, state_manager::StateManager,
};
use nvml_wrapper::Nvml;
use tokio::{signal::ctrl_c, sync::mpsc};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

#[tokio::main]
async fn main() -> Result<()> {
    logger::init_logging();

    // Parse the command line arguments
    let args_options = ArgsOptions::parse();

    // This token and tracker will be used to handle graceful shutdown
    let tracker = TaskTracker::new();
    let token = CancellationToken::new();

    // Use thin channel to move errors to the state task
    // to later transmit then to the D-Bus
    let (tx_err, rx_err) = mpsc::channel(16);

    // Start the configuration manager
    let (tx_config_manager, rx_config_manager) = mpsc::channel(16);
    {
        let token = token.clone();
        let tx_err = tx_err.clone();

        tracker.spawn(async move {
            let mut config_manager =
                ConfigManager::new(&args_options.config_file_path);

            config_manager.run(token, rx_config_manager, tx_err).await;
        });
    }

    // NVML is thread-safe so it is safe to make
    // simultaneous NVML calls from multiple threads.
    // We can therefore simply wrap it in a Arc with no Mutex
    let nvml =
        Arc::new(Nvml::init().with_context(|| "Failed to load NVML library")?);

    // Start the fan speed manager
    let (tx_fan_manager, rx_fan_manager) = mpsc::channel(16);
    {
        let nvml = nvml.clone();
        let token = token.clone();
        let tx_err = tx_err.clone();

        tracker.spawn(async move {
            let mut fan_manager = FanManager::new(nvml);
            fan_manager.run(token, rx_fan_manager, tx_err).await;
        });
    }

    // Start the state manager
    {
        let nvml = nvml.clone();
        let token = token.clone();

        tracker.spawn(async move {
            let mut state_manager = StateManager::new(
                nvml,

                tx_fan_manager, 
                tx_config_manager
            );

            state_manager.run(token, rx_err).await;
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
