use anyhow::Result;

use crate::{
    fan_curve::{FanCurve, fan_mode::FanMode},
    gpu_config::GpuConfig,
    gpu_data::{GpuData, GpuVendorData},
    gpu_info::{GpuInfo, GpuVendorInfo},
};

pub enum GpuVendor {
    Nvidia,
    AMD,
    Intel,
}

// GPU device trait
pub trait GpuDevice {
    // Return the device vendor
    fn get_vendor() -> GpuVendor;

    // Set the device fan curve, this does not automatically
    // set the fan mode to curve
    fn set_fan_curve(fan_curve: Box<dyn FanCurve + Send>);
    // Set the device fan mode, if no fan curve was previously set
    // default to a 100% fan speed curve
    fn set_fan_mode(fan_mode: FanMode) -> Result<()>;

    // Return the device vendor specific information
    fn get_vendor_info() -> GpuVendorInfo;
    // Return the device general information
    fn get_gpu_info() -> GpuInfo;

    // Return the device vendor specific real time data,
    // the update frequency is controlled by the set_update_freq function,
    // the default update frequency is 1 hertz
    fn get_vendor_data() -> GpuVendorData;
    // Return the device general real time data
    // the update frequency is controlled by the set_update_freq function,
    // the default update frequency is 1 hertz
    fn get_gpu_data() -> GpuData;

    // Apply the given GPU configuration to the device
    // The configuration vendor must match the
    fn apply_gpu_config(gpu_config: GpuConfig) -> Result<()>;
}
