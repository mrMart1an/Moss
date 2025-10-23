use std::sync::Arc;

use anyhow::{Context, Ok, Result};
use nvml_wrapper::{Device, Nvml, enum_wrappers::device::TemperatureThreshold};

use crate::{
    fan_curve::{FanCurve, fan_mode::FanMode},
    gpu_config::GpuConfig,
    gpu_data::{GpuData, GpuVendorData},
    gpu_device::{GpuDevice, GpuVendor},
    gpu_info::{GpuInfo, GpuVendorInfo},
};

pub struct NvidiaDevice {
    // Store a reference to the NVML context
    nvml: Arc<Nvml>,

    // Store the GPU unique identifier
    uuid: String,

    // Store the device generic and vendor specific informations
    gpu_info: GpuInfo,
    gpu_vendor_info: GpuVendorInfo,
}

impl NvidiaDevice {
    pub fn new(nvml: &Arc<Nvml>, uuid: &str) -> Result<Self> {
        let device = nvml.device_by_uuid(uuid).with_context(|| {
            format!("Failed to retrive GPU device \"{}\"", uuid)
        })?;

        // Obtain the device informations
        let gpu_info = Self::get_gpu_info(&device).with_context(|| {
            format!("Failed to retrive GPU info for \"{}\"", uuid)
        })?;
        let gpu_vendor_info =
            Self::get_gpu_vendor_info(nvml.sys_driver_version()?, &device)
                .with_context(|| {
                    format!(
                        "Failed to retrive GPU vendor info for \"{}\"",
                        uuid
                    )
                })?;

        Ok(Self {
            nvml: nvml.clone(),
            uuid: uuid.to_string(),
            gpu_info,
            gpu_vendor_info,
        })
    }

    // Return a NVML device handle.
    // This function can fail and return an error
    fn get<'a>(&'a self) -> Result<Device<'a>> {
        let uuid = self.uuid.as_str();

        self.nvml.device_by_uuid(uuid).with_context(|| {
            format!("Failed to retrive GPU device \"{}\"", uuid)
        })
    }

    // Return a reference to the NVML handle
    fn nvml(&self) -> Arc<Nvml> {
        self.nvml.clone()
    }

    fn get_gpu_info<'a, 'b>(device: &'a Device<'b>) -> Result<GpuInfo> {
        let power_limit_constraints =
            device.power_management_limit_constraints()?;

        Ok(GpuInfo {
            uuid: device.uuid()?,
            name: device.name()?,
            pcie_width: device.current_pcie_link_width()?,
            pcie_gen: device.current_pcie_link_gen()?,
            power_limit_max: power_limit_constraints.max_limit,
            power_limit_min: power_limit_constraints.min_limit,
            power_limit_default: device.power_management_limit_default()?,
        })
    }

    fn get_gpu_vendor_info<'a, 'b>(
        driver_version: String,
        device: &'a Device<'b>,
    ) -> Result<GpuVendorInfo> {
        Ok(GpuVendorInfo::Nvidia {
            driver_version: driver_version,
            vbios: device.vbios_version()?,
            cuda_core_count: device.num_cores()?,
            max_temp: device
                .temperature_threshold(TemperatureThreshold::GpuMax)?,
            mem_max_temp: device
                .temperature_threshold(TemperatureThreshold::MemoryMax)?,
            slowdown_temp: device
                .temperature_threshold(TemperatureThreshold::Slowdown)?,
            shutdown_temp: device
                .temperature_threshold(TemperatureThreshold::Shutdown)?,
        })
    }
}

impl GpuDevice for NvidiaDevice {
    // Return the device vendor
    fn get_vendor() -> GpuVendor {
        GpuVendor::Nvidia
    }

    // Set the device fan curve, this does not automatically
    // set the fan mode to curve
    fn set_fan_curve(&self, fan_curve: Box<dyn FanCurve + Send>) {
        todo!();
    }
    // Set the device fan mode, if no fan curve was previously set
    // default to a 100% fan speed curve
    fn set_fan_mode(&self, fan_mode: FanMode) -> Result<()> {
        todo!();
    }
    // Update the fan speed according to the mode and the fan curve
    fn update_fan(&self) {
        todo!();
    }

    // Return the device vendor specific information
    fn get_vendor_info(&self) -> GpuVendorInfo {
        self.gpu_vendor_info.clone()
    }
    // Return the device general information
    fn get_gpu_info(&self) -> GpuInfo {
        self.gpu_info.clone()
    }

    // Return the device vendor specific real time data,
    // the update frequency is controlled by the set_update_freq function,
    // the default update frequency is 1 hertz
    fn get_vendor_data(&self) -> GpuVendorData {
        todo!();
    }
    // Return the device general real time data
    // the update frequency is controlled by the set_update_freq function,
    // the default update frequency is 1 hertz
    fn get_gpu_data(&self) -> GpuData {
        todo!();
    }

    // Apply the given GPU configuration to the device
    // The configuration vendor must match the
    fn apply_gpu_config(&self, gpu_config: GpuConfig) -> Result<()> {
        todo!();
    }
}
