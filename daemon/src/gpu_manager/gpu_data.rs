use nvml_wrapper::{
    Device,
    enum_wrappers::device::{
        Clock, ClockId, TemperatureSensor,
        TemperatureThreshold,
    },
};
use tracing::warn;

#[derive(Debug, Default)]
pub struct DeviceData {
    // GPU information
    pub name: String,
    pub uuid: String,

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
    pub power_limit_max: Option<u32>,
    pub power_limit_min: Option<u32>,
    pub power_limit_default: Option<u32>,

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
    pub fn update(&mut self, device: &Device) {
        // Get device UUID
        if let Ok(uuid) = device.uuid() {
            // If the old UUID is set and is the same as
            // the first UUID skip fetching the identical information
            if self.uuid != uuid {
                self.uuid = uuid;

                if let Ok(name) = device.name() {
                    self.name = name;
                }

                if let Ok(vbios) = device.vbios_version() {
                    self.vbios = vbios;
                }

                self.num_cores = device.num_cores().ok();

                self.pcie_width = device.current_pcie_link_width().ok();
                self.pcie_gen = device.current_pcie_link_gen().ok();
            }
        }

        // Get temperature data
        if let Ok(temp) = device.temperature(TemperatureSensor::Gpu) {
            self.temp_gpu = Some(temp);
        } else {
            self.temp_gpu = None;

            warn!("Failed to fetch GPU temperature");
        }

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

        // Get utilization info
        if let Ok(utilization) = device.utilization_rates() {
            self.core_usage = Some(utilization.gpu);
            self.mem_usage = Some(utilization.memory);
        } else {
            self.core_usage = None;
            self.mem_usage = None;

            warn!("Failed to fetch GPU utilization info");
        }

        // Get utilization info
        if let Ok(limits) = device.power_management_limit_constraints() {
            self.power_limit_max = Some(limits.max_limit);
            self.power_limit_min = Some(limits.min_limit);
        } else {
            self.power_limit_max = None;
            self.power_limit_min = None;

            warn!("Failed to fetch GPU power limit info");
        }

        // Power usage information
        self.power_usage = device.power_usage().ok();
        self.power_limit = device.power_management_limit().ok();
        self.power_limit_default = device.power_management_limit_default().ok();

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

        // Fan information
        self.fan_speed = device.fan_speed(0).ok();
        self.fan_speed_rpm = device.fan_speed_rpm(0).ok();

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
    }
}
