// GPU data is information that is update in real time

// Store the vendor specific GPU data
#[derive(Debug, Clone)]
pub enum GpuVendorData {
    Nvidia {
        sm_freq: Option<u32>,
        video_freq: Option<u32>,

        // Core and memory max boost frequency
        graphics_boost_freq: Option<u32>,
        mem_boost_freq: Option<u32>,
        sm_boost_freq: Option<u32>,
        video_boost_freq: Option<u32>,
    },
    AMD {
        // TODO: AMD vendor data
    },
}

// Store the general GPU data
#[derive(Debug, Clone)]
pub struct GpuData {
    pub temp_gpu: u32,

    // Core and memory current frequency
    pub graphics_freq: u32,
    pub mem_freq: u32,

    // Overclocking frequency offsets
    pub core_clock_offset: i32,
    pub mem_clock_offset: i32,

    // Power usage and power limit
    pub power_usage: u32,
    pub power_limit: u32,

    // Fan information
    pub fan_speed: u32,
    pub fan_speed_rpm: u32,

    // Utilization information
    pub core_usage: u32,
    pub mem_usage: u32,

    // Memory utilization information, all values in bytes
    pub total_memory: u64,
    pub used_memory: u64,
    pub free_memory: u64,
}
