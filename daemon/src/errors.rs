use thiserror::Error;

use crate::{config_manager::ConfigError, devices_manager::DeviceManagerError};

// The main daemon error type
#[derive(Debug, Error)]
pub enum MossdError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    DeviceManager(#[from] DeviceManagerError),
}
