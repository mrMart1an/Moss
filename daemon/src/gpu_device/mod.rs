pub mod nvidia_device;

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
}

// GPU device trait
pub trait GpuDevice {
    // Return the device vendor
    fn get_vendor() -> GpuVendor;

    // Set the device fan curve, this does not automatically
    // set the fan mode to curve
    fn set_fan_curve(&self, fan_curve: Box<dyn FanCurve + Send>);
    // Set the device fan mode, if no fan curve was previously set
    // default to a 100% fan speed curve
    fn set_fan_mode(&self, fan_mode: FanMode) -> Result<()>;
    // Update the fan speed according to the mode and the fan curve
    fn update_fan(&self);

    // Return the device vendor specific information
    fn get_vendor_info(&self) -> GpuVendorInfo;
    // Return the device general information
    fn get_gpu_info(&self) -> GpuInfo;

    // Return the device vendor specific real time data,
    // the update frequency is controlled by the set_update_freq function,
    // the default update frequency is 1 hertz
    fn get_vendor_data(&self) -> GpuVendorData;
    // Return the device general real time data
    // the update frequency is controlled by the set_update_freq function,
    // the default update frequency is 1 hertz
    fn get_gpu_data(&self) -> GpuData;

    // Apply the given GPU configuration to the device
    // The configuration vendor must match the
    fn apply_gpu_config(&self, gpu_config: GpuConfig) -> Result<()>;
}
