use anyhow::Result;
use mossd::{
    arg_parser::ArgsOptions, config_manager::ConfigManager,
    dbus_service::DBusService, devices_manager::DevicesManager, logger,
};
use tokio::{
    select,
    signal::unix::{signal, Signal, SignalKind},
    sync::{broadcast, mpsc},
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

#[tokio::main]
async fn main() -> Result<()> {
    // The D-Bus layer will send each log onto this channel
    let (tx_log, rx_log) = mpsc::channel(100);
    logger::init_logging(tx_log);

    // Parse the command line arguments
    let args_options = ArgsOptions::parse();

    // This token and tracker will be used to handle graceful shutdown
    let tracker = TaskTracker::new();
    let token = CancellationToken::new();

    // Start the configuration manager
    let (tx_config_manager, rx_config_manager) = mpsc::channel(16);
    {
        let token = token.clone();

        tracker.spawn(async move {
            let mut config_manager =
                ConfigManager::new(args_options.config_file_path);

            config_manager.run(token, rx_config_manager).await;
        });
    }

    // Start the device manager
    let (tx_devices_manager, rx_devices_manager) = mpsc::channel(16);
    let (tx_devices_notification, rx_devices_notification) =
        broadcast::channel(16);
    {
        let token = token.clone();
        let tx_config_manager = tx_config_manager.clone();

        tracker.spawn(async move {
            let mut devices_manager = DevicesManager::new();
            devices_manager
                .run(
                    token,
                    rx_devices_manager,
                    tx_devices_notification,
                    tx_config_manager,
                )
                .await;
        });
    }

    // Start the D-Bus service
    {
        let token = token.clone();

        let tx_devices_manager = tx_devices_manager.clone();
        let tx_config_manager = tx_config_manager.clone();

        tracker.spawn(async move {
            let mut dbus_service = DBusService::new();
            dbus_service
                .run(
                    token,
                    tx_devices_manager,
                    tx_config_manager,
                    rx_devices_notification,
                    rx_log,
                )
                .await;
        });
    }

    // Handle shutdown signals
    let mut sigint = signal(SignalKind::interrupt()).unwrap();
    let mut sigterm = signal(SignalKind::terminate()).unwrap();

    select! {
        _ = sigint.recv() => { },
        _ = sigterm.recv() => { },
    }

    // Cancel the token to communicate the program
    // termination to the running tasks
    token.cancel();

    // Wait for the tasks to finish
    tracker.close();
    tracker.wait().await;

    Ok(())
}
