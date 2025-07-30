use std::{sync::Arc, time::Duration};

use anyhow::{Result, anyhow};
use nvml_wrapper::{
    Device, Nvml,
    enum_wrappers::device::{
        Clock, ClockId, TemperatureSensor, TemperatureThreshold,
    },
};
use tokio::time::Instant;
use tracing::warn;

use crate::device::GpuDevice;

#[derive(Default, Debug)]
pub struct DeviceData {
    // Store a reference to the GPU for more convenient data access 
    gpu_device: Option<GpuDevice>,
    last_update: Option<Instant>,

    // GPU information
    pub uuid: String,
    pub name: String,

    pub driver_version: String,
    pub vbios: String,
    pub num_cores: Option<u32>,
    pub pcie_width: Option<u32>,
    pub pcie_gen: Option<u32>,

    // Store the temperature for each sensor on the GPU
    pub temp_gpu: Option<u32>,

    // Core and memory current frequency
    pub graphics_freq: Option<u32>,
    pub video_freq: Option<u32>,
    pub sm_freq: Option<u32>,
    pub mem_freq: Option<u32>,

    // Core and memory max boost frequency
    pub graphics_boost_freq: Option<u32>,
    pub video_boost_freq: Option<u32>,
    pub sm_boost_freq: Option<u32>,
    pub mem_boost_freq: Option<u32>,

    // Overclocking frequency offsets
    pub core_clock_offset: Option<i32>,
    pub mem_clock_offset: Option<i32>,

    // Power usage and power limit
    pub power_usage: Option<u32>,
    pub power_limit: Option<u32>,
    pub power_limit_max: u32,
    pub power_limit_min: u32,
    pub power_limit_default: u32,

    // Fan information
    pub fan_speed: Option<u32>,
    pub fan_speed_rpm: Option<u32>,

    // Utilization information
    pub core_usage: Option<u32>,
    pub mem_usage: Option<u32>,

    // Memory utilization information, all values in bytes
    pub total_memory: Option<u64>,
    pub used_memory: Option<u64>,
    pub free_memory: Option<u64>,

    // GPU temperature threshold
    pub max_temp: Option<u32>,
    pub mem_max_temp: Option<u32>,
    pub slowdown_temp: Option<u32>,
    pub shutdown_temp: Option<u32>,
}

impl DeviceData {
    pub fn new(gpu_device: &GpuDevice) -> Result<Self> {
        let mut data = Self::default();

        // Store the GPU device in the struct and get a NVML device handle
        data.gpu_device = Some(gpu_device.clone());
        let device = gpu_device.get()?;

        // Update the device for the first time
        data.get_static_data(&gpu_device.nvml(), &device)?;
        data.update(Duration::from_secs(1));

        Ok(data)
    }

    // Update all of the device data
    pub fn update(&mut self, update_rate: Duration) {
        let elapsed = if let Some(last_update) = self.last_update {
            last_update.elapsed()
        } else {
            self.last_update = Some(Instant::now());
            self.last_update.unwrap().elapsed()
        };

        // Update only if necessary
        if elapsed >= update_rate {
            if let Some(gpu_device) = self.gpu_device.clone() {
                if let Ok(device) = gpu_device.get() {
                    self.update_temp(&device);
                    self.update_memory_info(&device);
                    self.update_utilization(&device);
                    self.update_power(&device);
                    self.update_frequency(&device);
                    self.update_frequency(&device);
                    self.update_fan(&device);
                }
            }
        }
    }

    fn get_static_data(
        &mut self,
        nvml: &Arc<Nvml>,
        device: &Device,
    ) -> Result<()> {
        // Store the driver version
        self.driver_version = nvml.sys_nvml_version()?;

        // Get device UUID
        self.uuid = device.uuid()?;
        self.name = device.name()?;

        self.vbios = device.vbios_version()?;

        self.num_cores = Some(device.num_cores()?);

        self.pcie_width = Some(device.current_pcie_link_width()?);
        self.pcie_gen = Some(device.current_pcie_link_gen()?);

        // Get power limit info
        if let Ok(limits) = device.power_management_limit_constraints() {
            self.power_limit_max = limits.max_limit;
            self.power_limit_min = limits.min_limit;

            Ok(())
        } else {
            Err(anyhow!("Failed to fetch power limit constrains"))
        }?;

        self.power_limit_default = device.power_management_limit_default()?;

        // Temperature threshold
        self.max_temp = device
            .temperature_threshold(TemperatureThreshold::GpuMax)
            .ok();
        self.mem_max_temp = device
            .temperature_threshold(TemperatureThreshold::MemoryMax)
            .ok();
        self.slowdown_temp = device
            .temperature_threshold(TemperatureThreshold::Slowdown)
            .ok();
        self.shutdown_temp = device
            .temperature_threshold(TemperatureThreshold::Shutdown)
            .ok();

        Ok(())
    }

    fn update_temp(&mut self, device: &Device) {
        // Get temperature data
        if let Ok(temp) = device.temperature(TemperatureSensor::Gpu) {
            self.temp_gpu = Some(temp);
        } else {
            self.temp_gpu = None;

            warn!("Failed to fetch GPU temperature");
        }
    }

    fn update_memory_info(&mut self, device: &Device) {
        // Get memory info
        if let Ok(mem_info) = device.memory_info() {
            self.total_memory = Some(mem_info.total);
            self.used_memory = Some(mem_info.used);
            self.free_memory = Some(mem_info.free);
        } else {
            self.total_memory = None;
            self.used_memory = None;
            self.free_memory = None;

            warn!("Failed to fetch GPU memory info");
        }
    }

    fn update_utilization(&mut self, device: &Device) {
        // Get utilization info
        if let Ok(utilization) = device.utilization_rates() {
            self.core_usage = Some(utilization.gpu);
            self.mem_usage = Some(utilization.memory);
        } else {
            self.core_usage = None;
            self.mem_usage = None;

            warn!("Failed to fetch GPU utilization info");
        }
    }

    fn update_power(&mut self, device: &Device) {
        // Power usage information
        self.power_usage = device.power_usage().ok();
        self.power_limit = device.power_management_limit().ok();
    }

    fn update_frequency(&mut self, device: &Device) {
        // Update current clocks
        self.graphics_freq =
            device.clock(Clock::Graphics, ClockId::Current).ok();
        self.video_freq = device.clock(Clock::Video, ClockId::Current).ok();
        self.sm_freq = device.clock(Clock::SM, ClockId::Current).ok();
        self.mem_freq = device.clock(Clock::Memory, ClockId::Current).ok();

        // Update boosts clocks
        self.graphics_boost_freq = device.max_clock_info(Clock::Graphics).ok();
        self.video_boost_freq = device.max_clock_info(Clock::Video).ok();
        self.sm_boost_freq = device.max_clock_info(Clock::SM).ok();
        self.mem_boost_freq = device.max_clock_info(Clock::Memory).ok();

        // Get the overclocking current offsets
        self.mem_clock_offset = device.mem_clock_vf_offset().ok();
        self.core_clock_offset = device.gpc_clock_vf_offset().ok();
    }

    fn update_fan(&mut self, device: &Device) {
        // Fan information
        self.fan_speed = device.fan_speed(0).ok();
        self.fan_speed_rpm = device.fan_speed_rpm(0).ok();
    }
}
