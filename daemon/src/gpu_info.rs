// GPU info are static information that are query only once

// Store vendor specific information
pub enum GpuVendorInfo {
    Nvidia {
        driver_version: String,
        vbios: String,

        cuda_core_count: u32,

        // GPU temperature threshold
        max_temp: Option<u32>,
        mem_max_temp: Option<u32>,
        slowdown_temp: Option<u32>,
        shutdown_temp: Option<u32>,
    },
    AMD {
        // TODO: AMD vendor info
    },
}

// Store GPU general information
pub struct GpuInfo {
    pub uuid: String,
    pub name: String,

    pub pcie_width: u32,
    pub pcie_gen: u32,

    pub power_limit_max: u32,
    pub power_limit_min: u32,
    pub power_limit_default: u32,
}

