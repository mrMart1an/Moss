use std::sync::Arc;

use anyhow::{Context, Result};
use nvml_wrapper::{Device, Nvml};

// Store a NVML GPU device and the original NVML context
#[derive(Debug, Clone)]
pub struct GpuDevice {
    nvml: Arc<Nvml>,

    // Store the GPU unique identifier
    uuid: String,
}

impl GpuDevice {
    // Create a new GPU device
    pub fn new(nvml: &Arc<Nvml>, uuid: &str) -> Self {
        Self { 
            nvml: nvml.clone(),
            uuid: uuid.to_string() 
        }
    }

    // Return a device handle.
    // This function can fail and return an error
    pub fn get<'a>(&'a self) -> Result<Device<'a>> {
        let uuid = self.uuid.as_str();

        self.nvml
            .device_by_uuid(uuid)
            .with_context(
                ||
                format!("Failed to retrive GPU device \"{}\"", uuid)
            )
    }

    // Return a reference to the NVML handle
    pub fn nvml(&self) -> Arc<Nvml> {
        self.nvml.clone()
    }
}
