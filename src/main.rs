use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use moss_gpu::{
    arg_parser::ArgsOptions, config_manager::ConfigManager, fan_curve::{hysteresis_curve::HysteresisCurve, linear_curve::LinearCurve, FanCurve}, fan_manager::{FanManager, FanMessage}, logger
};
use nvml_wrapper::Nvml;
use tokio::{select, signal::ctrl_c, sync::mpsc};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use tracing::error;

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
    let (tx_err, mut _rx_err) = mpsc::channel(16);

    // Start the configuration manager
    let (_tx_config_manager, rx_config_manager) = mpsc::channel(16);
    {
        let token = token.clone();
        let tx_err = tx_err.clone();

        tracker.spawn(async move {
            let mut config_manager = ConfigManager::new(
                &args_options.config_file_path
            );

            config_manager.run(token, rx_config_manager, tx_err).await;
        });
    }

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
        let tx_err = tx_err.clone();

        tracker.spawn(async move {
            let mut fan_manager = FanManager::new(nvml);
            fan_manager.run(token, rx_fan_manager, tx_err).await;
        });
    }

    // NOTE: test code --------
    let uuid = nvml.device_by_index(0)?.uuid()?; 

    let mut curve = HysteresisCurve::new(LinearCurve::new(), 2, 2);
    curve.add_point((40, 40).into());
    curve.add_point((50, 50).into());
    curve.add_point((60, 80).into());
    curve.add_point((75, 100).into());

    _tx_fan_manager.send(FanMessage::UpdateCurve { 
        uuid: uuid.clone(),
        new_curve: Box::new(curve), 
    }).await.map_err(|_| {
        anyhow!("Fan manager send error")
    })?;

    _tx_fan_manager.send(FanMessage::SetMode { 
        uuid: uuid.clone(),
        mode: moss_gpu::fan_manager::FanMode::Manual 
    }).await.map_err(|_| {
        anyhow!("Fan manager send error")
    })?;


    //if let Some(data) = _rx_err.recv().await {
    //    for msg in data.chain() {
    //        error!("{}", msg);
    //    }
    //}


    // TODO: Handle different unix signal for graceful termination
    loop {

        select! {
            _ = ctrl_c() => { break; },
            err_msg = _rx_err.recv() => {
                if let Some(err) = err_msg {
                    for e in err.chain() {
                        error!("{e}");
                    }
                }
            }
        }
    }

    // NOTE: test code --------

    // Cancel the token to communicate the program
    // termination to the running tasks
    token.cancel();

    // Wait for the tasks to finish
    tracker.close();
    tracker.wait().await;

    Ok(())
}
