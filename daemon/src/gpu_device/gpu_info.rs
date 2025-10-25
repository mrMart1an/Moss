// GPU info are static information that are query only once

// Store vendor specific information
#[derive(Debug, Clone)]
pub enum GpuVendorInfo {
    Nvidia {
        driver_version: String,
        vbios: String,

        cuda_core_count: u32,

        // GPU temperature threshold
        max_temp: u32,
        mem_max_temp: u32,
        slowdown_temp: u32,
        shutdown_temp: u32,
    },
    AMD {
        // TODO: AMD vendor info
    },
}

// Store GPU general information
#[derive(Debug, Clone)]
pub struct GpuInfo {
    pub uuid: String,
    pub name: String,

    pub pcie_width: u32,
    pub pcie_gen: u32,

    pub power_limit_max: u32,
    pub power_limit_min: u32,
    pub power_limit_default: u32,
}

