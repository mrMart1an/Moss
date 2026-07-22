// GPU data is information that is update in real time

// Store the vendor specific GPU data
#[derive(Default, Debug, Clone, Copy)]
pub enum GpuVendorData {
    #[default]
    None,

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

// Store the vendor specific GPU data updates since last query
#[derive(Default, Debug, Clone, Copy)]
pub enum GpuVendorDataUpdates {
    #[default]
    None,

    Nvidia {
        sm_freq: bool,
        video_freq: bool,

        // Core and memory max boost frequency
        graphics_boost_freq: bool,
        mem_boost_freq: bool,
        sm_boost_freq: bool,
        video_boost_freq: bool,
    },
    AMD {
        // TODO: AMD vendor data
    },
}

// Store the general GPU data
#[derive(Default, Debug, Clone, Copy)]
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

// Report the update status of each variable since the last query
#[derive(Debug, Default, Clone, Copy)]
pub struct GpuDataUpdates {
    pub temp_gpu: bool,

    // Core and memory current frequency
    pub graphics_freq: bool,
    pub mem_freq: bool,

    // Overclocking frequency offsets
    pub core_clock_offset: bool,
    pub mem_clock_offset: bool,

    // Power usage and power limit
    pub power_usage: bool,
    pub power_limit: bool,

    // Fan information
    pub fan_speed: bool,
    pub fan_speed_rpm: bool,

    // Utilization information
    pub core_usage: bool,
    pub mem_usage: bool,

    // Memory utilization information, all values in bytes
    pub total_memory: bool,
    pub used_memory: bool,
    pub free_memory: bool,
}

impl GpuData {
    pub fn updated_from(&self, from: &GpuData) -> GpuDataUpdates {
        GpuDataUpdates {
            temp_gpu: self.temp_gpu != from.temp_gpu,

            graphics_freq: self.graphics_freq != from.graphics_freq,
            mem_freq: self.mem_freq != from.mem_freq,

            core_clock_offset: self.core_clock_offset != from.core_clock_offset,
            mem_clock_offset: self.mem_clock_offset != from.mem_clock_offset,

            power_usage: self.power_usage != from.power_usage,
            power_limit: self.power_limit != from.power_limit,

            fan_speed: self.fan_speed != from.fan_speed,
            fan_speed_rpm: self.fan_speed_rpm != from.fan_speed_rpm,

            core_usage: self.core_usage != from.core_usage,
            mem_usage: self.mem_usage != from.mem_usage,

            total_memory: self.total_memory != from.total_memory,
            used_memory: self.used_memory != from.used_memory,
            free_memory: self.free_memory != from.free_memory,
        }
    }
}

impl GpuVendorData {
    pub fn updated_from(&self, from: &GpuVendorData) -> GpuVendorDataUpdates {
        match self {
            GpuVendorData::Nvidia {
                sm_freq,
                video_freq,
                graphics_boost_freq,
                mem_boost_freq,
                sm_boost_freq,
                video_boost_freq,
            } => {
                if let GpuVendorData::Nvidia {
                    sm_freq: from_sm_freq,
                    video_freq: from_video_freq,
                    graphics_boost_freq: from_graphics_boost_freq,
                    mem_boost_freq: from_mem_boost_freq,
                    sm_boost_freq: from_sm_boost_freq,
                    video_boost_freq: from_video_boost_freq,
                } = from
                {
                    GpuVendorDataUpdates::Nvidia {
                        sm_freq: sm_freq != from_sm_freq,
                        video_freq: video_freq != from_video_freq,
                        graphics_boost_freq: graphics_boost_freq
                            != from_graphics_boost_freq,
                        mem_boost_freq: mem_boost_freq != from_mem_boost_freq,
                        sm_boost_freq: sm_boost_freq != from_sm_boost_freq,
                        video_boost_freq: video_boost_freq
                            != from_video_boost_freq,
                    }
                } else {
                    GpuVendorDataUpdates::default()
                }
            }
            _ => {
                GpuVendorDataUpdates::default()
            }
        }
    }
}
