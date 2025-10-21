// Vendor specific configuration
pub enum VendorConfig {
    Nvidia {
        core_clock_offset: u32,
        mem_clock_offset: u32,
    },
    AMD {
        // TODO: AMD GPU config
    },
    Intel {
        // TODO: Intel GPU config
    },
}

// General configuration
pub struct GpuConfig {
    pub vendor_config: VendorConfig,

    // GPU power limit
    pub power_limit: u32,
}
