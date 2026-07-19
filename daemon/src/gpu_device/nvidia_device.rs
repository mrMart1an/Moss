use std::sync::Arc;

use anyhow::{Context, anyhow};
use nvml_wrapper::{
    Device, Nvml,
    enum_wrappers::device::{
        Clock, ClockId, TemperatureSensor, TemperatureThreshold,
    },
    enums::device::FanControlPolicy,
    error::NvmlError,
};
use tracing::{debug, warn};

use crate::{
    fan_curve::{FanCurve, fan_mode::FanMode},
    gpu_device::{
        GpuDevice, GpuVendor, Result,
        gpu_config::{GpuConfig, GpuVendorConfig},
        gpu_data::{
            GpuData, GpuDataUpdates, GpuVendorData, GpuVendorDataUpdates,
        },
        gpu_info::{GpuInfo, GpuVendorInfo},
    },
};

pub struct NvidiaDevice {
    // Store a reference to the NVML context
    nvml: Arc<Nvml>,

    // Store the GPU unique identifier
    uuid: String,

    // Store the device generic and vendor specific informations
    gpu_info: GpuInfo,
    gpu_vendor_info: GpuVendorInfo,

    // Store the device generic and vendor specific last data
    gpu_data_last: Option<GpuData>,
    gpu_vendor_data_last: Option<GpuVendorData>,

    // Store the current fan mode
    fan_mode: FanMode,
    // Fan curve to apply in curve mode
    fan_curve: Option<Box<dyn FanCurve + Send>>,
}

impl NvidiaDevice {
    pub fn new(nvml: Arc<Nvml>, uuid: &str) -> Result<Self> {
        let device = nvml.device_by_uuid(uuid).with_context(|| {
            format!("Failed to retrive GPU device \"{}\"", uuid)
        })?;

        // Obtain the device informations
        let gpu_info = Self::get_gpu_info(&device).with_context(|| {
            format!("Failed to retrive GPU info for \"{}\"", uuid)
        })?;

        let driver_version = nvml.sys_driver_version().with_context(|| {
            format!("Failed to retrive GPU driver version for \"{}\"", uuid)
        })?;

        let gpu_vendor_info = Self::get_gpu_vendor_info(
            driver_version,
            &device,
        )
        .with_context(|| {
            format!("Failed to retrive GPU vendor info for \"{}\"", uuid)
        })?;

        // Obtain the initialization general and vendor specific data
        let gpu_data_last = None;
        let gpu_vendor_data_last = None;

        // Determine the current fan mode
        // We can't just assume it is automatic, if an old instance of
        // the program changed it and crashed if could still be manual
        // TODO: Handle multiple fan
        let control_policy =
            device.fan_control_policy(0).with_context(|| {
                format!("Failed to retrive fan control policy for \"{}\"", uuid)
            })?;

        let fan_mode =
            if control_policy == FanControlPolicy::TemperatureContinousSw {
                FanMode::Auto
            } else {
                let current_speed = device.fan_speed(0)?;

                FanMode::Manual(current_speed as u8)
            };

        Ok(Self {
            nvml: nvml.clone(),
            uuid: uuid.to_string(),

            gpu_info,
            gpu_vendor_info,

            gpu_data_last,
            gpu_vendor_data_last,

            fan_mode,
            fan_curve: None,
        })
    }

    // Return a NVML device handle.
    // This function can fail and return an error
    fn get_device<'a>(&'a self) -> Result<Device<'a>> {
        let uuid = self.uuid.as_str();

        self.nvml.device_by_uuid(uuid).with_context(|| {
            format!("Failed to retrive GPU device \"{}\"", uuid)
        })
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
            max_temp: Self::ok_support(
                device.temperature_threshold(TemperatureThreshold::GpuMax),
            )?,
            mem_max_temp: Self::ok_support(
                device.temperature_threshold(TemperatureThreshold::MemoryMax),
            )?,
            slowdown_temp: Self::ok_support(
                device.temperature_threshold(TemperatureThreshold::Slowdown),
            )?,
            shutdown_temp: Self::ok_support(
                device.temperature_threshold(TemperatureThreshold::Shutdown),
            )?,
        })
    }

    fn get_gpu_data(&mut self) -> Result<(GpuData, GpuDataUpdates)> {
        let device = self.get_device()?;

        // Get the fan speed data
        // TODO: Handle multiples fans
        let mut fan_speed = 0;
        let mut fan_speed_rpm = 0;

        if device.num_fans()? > 0 {
            fan_speed = device.fan_speed(0)?;
            fan_speed_rpm = device.fan_speed_rpm(0)?;
        }

        // Get the core and memory usage data
        let mut core_usage = 0;
        let mut mem_usage = 0;

        if let Ok(utilization) = device.utilization_rates() {
            core_usage = utilization.gpu;
            mem_usage = utilization.memory;
        } else {
            warn!("Failed to fetch GPU utilization info");
        }

        // Get the memory usage data
        let mut total_memory = 0;
        let mut used_memory = 0;
        let mut free_memory = 0;

        if let Ok(mem_info) = device.memory_info() {
            total_memory = mem_info.total;
            used_memory = mem_info.used;
            free_memory = mem_info.free;
        } else {
            warn!("Failed to fetch GPU memory info");
        }

        let gpu_data = GpuData {
            temp_gpu: device.temperature(TemperatureSensor::Gpu)?,

            graphics_freq: device.clock(Clock::Graphics, ClockId::Current)?,
            mem_freq: device.clock(Clock::Memory, ClockId::Current)?,

            core_clock_offset: device.gpc_clock_vf_offset()?,
            mem_clock_offset: device.mem_clock_vf_offset()?,

            power_usage: device.power_usage()?,
            power_limit: device.power_management_limit()?,

            fan_speed,
            fan_speed_rpm,

            core_usage,
            mem_usage,

            total_memory,
            used_memory,
            free_memory,
        };

        // Generate Updates data
        let updates = if let Some(last) = &self.gpu_data_last {
            gpu_data.updated_from(last)
        } else {
            gpu_data.updated_from(&GpuData::default())
        };

        // Update last data
        self.gpu_data_last = Some(gpu_data.clone());

        Ok((gpu_data, updates))
    }

    fn get_gpu_vendor_data(
        &mut self,
    ) -> Result<(GpuVendorData, GpuVendorDataUpdates)> {
        let device = self.get_device()?;

        let gpu_vendor_data = GpuVendorData::Nvidia {
            sm_freq: Self::ok_support(
                device.clock(Clock::SM, ClockId::Current),
            )?,
            video_freq: Self::ok_support(
                device.clock(Clock::Video, ClockId::Current),
            )?,

            graphics_boost_freq: Self::ok_support(
                device.clock(Clock::Graphics, ClockId::CustomerMaxBoost),
            )?,
            mem_boost_freq: Self::ok_support(
                device.clock(Clock::Memory, ClockId::CustomerMaxBoost),
            )?,
            sm_boost_freq: Self::ok_support(
                device.clock(Clock::Video, ClockId::CustomerMaxBoost),
            )?,
            video_boost_freq: Self::ok_support(
                device.clock(Clock::Video, ClockId::CustomerMaxBoost),
            )?,
        };

        // Generate Updates data
        let updates = if let Some(last) = &self.gpu_vendor_data_last {
            gpu_vendor_data.updated_from(last)
        } else {
            gpu_vendor_data.updated_from(&GpuVendorData::default())
        };

        // Update last data
        self.gpu_vendor_data_last = Some(gpu_vendor_data.clone());

        Ok((gpu_vendor_data, updates))
    }

    // If the given result is Ok(T) return Ok(Some(T))
    // If the given result is a non supported error return Ok(None)
    // If the given result is any other kind of error return Err(e)
    fn ok_support<T>(
        value: std::result::Result<T, NvmlError>,
    ) -> Result<Option<T>> {
        match value {
            Ok(v) => Ok(Some(v)),
            Err(NvmlError::NotSupported) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

impl GpuDevice for NvidiaDevice {
    // Return the device vendor
    fn get_vendor(&self) -> GpuVendor {
        GpuVendor::Nvidia
    }

    // Set the device fan curve, this does not automatically
    // set the fan mode to curve
    fn set_fan_curve(&mut self, fan_curve: Box<dyn FanCurve + Send>) {
        self.fan_curve = Some(fan_curve);
    }
    // Set the device fan mode, if no fan curve was previously set
    // default to a 100% fan speed curve
    fn set_fan_mode(&mut self, fan_mode: FanMode) -> Result<()> {
        match fan_mode {
            FanMode::Auto => {
                self.get_device()?
                    .set_fan_control_policy(
                        0,
                        FanControlPolicy::TemperatureContinousSw,
                    )
                    .with_context(|| {
                        format!(
                            "Failed to set fan mode to auto: \"{}\"",
                            self.uuid
                        )
                    })?;
            }

            FanMode::Curve => {
                if self.fan_curve.is_none() {
                    return Err(anyhow!(
                        "Attempting to set curve fan mode with no curve!"
                    ));
                }

                self.get_device()?
                    .set_fan_control_policy(0, FanControlPolicy::Manual)
                    .with_context(|| {
                        format!(
                            "Failed to set fan mode to curve: \"{}\"",
                            self.uuid
                        )
                    })?;

                self.update_fan()?;
            }

            FanMode::Manual(_) => {
                self.get_device()?
                    .set_fan_control_policy(0, FanControlPolicy::Manual)
                    .with_context(|| {
                        format!(
                            "Failed to set fan mode to manual: \"{}\"",
                            self.uuid
                        )
                    })?;

                self.update_fan()?;
            }
        }

        self.fan_mode = fan_mode;

        Ok(())
    }
    // Update the fan speed according to the mode and the fan curve
    fn update_fan(&mut self) -> Result<()> {
        // Get the NVML device
        let mut device = self
            .get_device()
            .with_context(|| "Device query error during fan update")?;

        match self.fan_mode {
            FanMode::Curve => {
                // Should be impossible for the fan curve to not
                // be set by this point
                let fan_curve = self.fan_curve.as_ref().unwrap();

                // If the query for the temperature fail return
                // 110 degrees for safety
                let temp = device
                    .temperature(TemperatureSensor::Gpu)
                    .unwrap_or_else(|_| 110) as i32;
                let fan_speed = fan_curve.get_speed(temp);

                debug!(
                    "Updating fan: Mode Curve - Temp: {:?} - Speed: {:?}%",
                    temp, fan_speed
                );

                device.set_fan_speed(0, fan_speed as u32).with_context(
                    || {
                        format!(
                            "Failed to set fan speed for device \"{}\"",
                            self.uuid
                        )
                    },
                )?;
            }
            FanMode::Manual(speed) => {
                debug!("Updating fan: Mode Manual - Speed: {:?}%", speed);

                device.set_fan_speed(0, speed as u32).with_context(
                    || {
                        format!(
                            "Failed to set fan speed for device \"{}\"",
                            self.uuid
                        )
                    },
                )?;
            }
            _ => {
                debug!("Updating fan: Mode Auto")
            }
        }

        Ok(())
    }

    // Return the device vendor specific information
    fn get_vendor_info(&self) -> GpuVendorInfo {
        self.gpu_vendor_info.clone()
    }
    // Return the device general information
    fn get_info(&self) -> GpuInfo {
        self.gpu_info.clone()
    }

    // Return the device vendor specific real time data,
    // the update frequency is controlled by the set_update_freq function,
    // the default update frequency is 1 hertz
    fn get_vendor_data(
        &mut self,
    ) -> Result<(GpuVendorData, GpuVendorDataUpdates)> {
        self.get_gpu_vendor_data()
    }
    // Return the device general real time data
    // the update frequency is controlled by the set_update_freq function,
    // the default update frequency is 1 hertz
    fn get_data(&mut self) -> Result<(GpuData, GpuDataUpdates)> {
        self.get_gpu_data()
    }

    // Apply the given GPU configuration to the device
    // The configuration vendor must match the
    fn set_device_config(&mut self, gpu_config: GpuConfig) -> Result<()> {
        // Get the NVML device
        let mut device = self.get_device()?;

        // Set the power limit
        if let Some(power_limit) = gpu_config.power_limit {
            if power_limit > self.gpu_info.power_limit_max {
                warn!(
                    "requested power limit is beyond max ({}), ingoring it",
                    self.gpu_info.power_limit_max
                );
            } else if power_limit < self.gpu_info.power_limit_min {
                warn!(
                    "requested power limit is bellow min ({}), ingoring it",
                    self.gpu_info.power_limit_min
                );
            } else {
                device.set_power_management_limit(power_limit)?;
            }
        }

        // Set vendor specific config
        if let GpuVendorConfig::Nvidia {
            core_clock_offset,
            mem_clock_offset,
        } = gpu_config.vendor_config
        {
            if let Some(offset) = core_clock_offset {
                device.set_gpc_clock_vf_offset(offset)?;
            }
            if let Some(offset) = mem_clock_offset {
                device.set_mem_clock_vf_offset(offset)?;
            }
        }

        Ok(())
    }
}
