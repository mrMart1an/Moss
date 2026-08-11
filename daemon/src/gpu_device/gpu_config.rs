use serde::{Deserialize, Serialize};

// General configuration
#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy)]
pub struct GpuConfig {
    pub nvidia_config: NvidiaConfig,

    // GPU power limit
    pub power_limit: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize,  Default, Clone, Copy)]
pub struct NvidiaConfig {
    pub core_clock_offset: Option<i32>,
    pub mem_clock_offset: Option<i32>,
}
