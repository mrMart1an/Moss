use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result};
use moss_nv::fan_curve::{
    FanCurve, hysteresis_curve::HysteresisCurve, linear_curve::LinearCurve,
};
use nvml_wrapper::{Nvml, enum_wrappers::device::TemperatureSensor};
use tokio::time;

#[tokio::main]
async fn main() -> Result<()> {
    let nvml = Nvml::init().with_context(|| "Failed to load NVML library")?;

    // Get the first GPU. You might want to iterate or select a specific one.
    let mut device = nvml.device_by_index(0)?;

    // Hood the signal handler
    let term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(
        signal_hook::consts::SIGTERM,
        Arc::clone(&term),
    )?;
    signal_hook::flag::register(
        signal_hook::consts::SIGINT,
        Arc::clone(&term),
    )?;

    let mut curve = HysteresisCurve::new(LinearCurve::new(), 3, 3);
    curve.add_point((70, 40).into());
    curve.add_point((60, 100).into());

    let mut interval = time::interval(Duration::from_secs(2));

    while !term.load(Ordering::Relaxed) {
        // Get current temperature
        let current_temp = device.temperature(TemperatureSensor::Gpu)?;

        let actual_fan_speed = device.fan_speed(0)?;

        println!(
            "Current GPU Temperature: {}°C, Actual Fan Speed: {}%",
            current_temp, actual_fan_speed
        );

        device.set_fan_speed(0, curve.get_speed(current_temp).get())?;

        interval.tick().await;
    }

    device.set_default_fan_speed(0)?;

    Ok(())
}
