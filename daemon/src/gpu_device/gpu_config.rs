use serde::{Deserialize, Serialize};

// Vendor specific configuration
#[derive(Debug, Serialize, Deserialize,  Default, Clone, Copy)]
pub enum GpuVendorConfig {
    #[default]
    None,

    Nvidia {
        core_clock_offset: Option<i32>,
        mem_clock_offset: Option<i32>,
    }
}

// General configuration
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct GpuConfig {
    pub vendor_config: GpuVendorConfig,

    // GPU power limit
    pub power_limit: Option<u32>,
}
