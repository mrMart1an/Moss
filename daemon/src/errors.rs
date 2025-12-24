use thiserror::Error;

use crate::{
    config_manager::ConfigError, dbus_service::DbusServiceError,
    devices_manager::DevicesManagerError, state_manager::StateManagerError,
};

// The main daemon error type
#[derive(Debug, Error)]
pub enum MossdError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    DevicesManager(#[from] DevicesManagerError),
    #[error(transparent)]
    StateManager(#[from] StateManagerError),
    #[error(transparent)]
    DBusService(#[from] DbusServiceError),
}

impl MossdError {
    // Return a numeric error code based on the error type
    pub fn error_code(&self) -> u32 {
        match self {
            MossdError::Config(..) => {
                1
            }
            MossdError::DevicesManager(..) => {
                2
            }
            MossdError::StateManager(..) => {
                3
            }
            MossdError::DBusService(..) => {
                4
            }
        }
    }
}
