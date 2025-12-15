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
