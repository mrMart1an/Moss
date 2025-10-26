pub mod gpu_config;
pub mod gpu_data;
pub mod gpu_info;

pub mod nvidia_device;

use std::time::Duration;

use anyhow::Result;

// Default update intervals
pub const DEFAULT_DATA_UPDATE_INTERVAL: Duration = Duration::from_secs(1);
pub const DEFAULT_FAN_UPDATE_INTERVAL: Duration = Duration::from_secs(2);

use crate::{
    fan_curve::{FanCurve, fan_mode::FanMode},
    gpu_device::{
        gpu_config::GpuConfig,
        gpu_data::{GpuData, GpuVendorData},
        gpu_info::{GpuInfo, GpuVendorInfo},
    },
};

pub enum GpuVendor {
    Nvidia,
    AMD,
}

// GPU device trait
pub trait GpuDevice {
    // Return the device vendor
    fn get_vendor(&self) -> GpuVendor;

    // Set the device fan curve, this does not automatically
    // set the fan mode to curve
    fn set_fan_curve(&mut self, fan_curve: Box<dyn FanCurve + Send>);
    // Set the device fan mode, if no fan curve was previously set
    // default to a 100% fan speed curve
    fn set_fan_mode(&mut self, fan_mode: FanMode) -> Result<()>;
    // Update the fan speed according to the mode and the fan curve
    // the default update frequency is 0.5 hertz
    fn update_fan(&mut self);

    // Return the device vendor specific information
    fn get_vendor_info(&self) -> GpuVendorInfo;
    // Return the device general information
    fn get_gpu_info(&self) -> GpuInfo;

    // Return the device vendor specific real time data,
    // the update frequency is controlled by the set_update_freq function,
    // the default update frequency is 1 hertz
    fn get_vendor_data(&mut self) -> GpuVendorData;
    // Return the device general real time data
    // the update frequency is controlled by the set_update_freq function,
    // the default update frequency is 1 hertz
    fn get_gpu_data(&mut self) -> GpuData;
    // Change the vendor and general data update interval
    fn set_data_update_interval(&mut self, update_interval: Duration);

    // Apply the given GPU configuration to the device
    // The configuration vendor must match the
    fn apply_gpu_config(&mut self, gpu_config: GpuConfig) -> Result<()>;
}
