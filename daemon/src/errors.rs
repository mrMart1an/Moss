use thiserror::Error;

use crate::{
    config_manager::ConfigError, devices_manager::DevicesManagerError,
    state_manager::StateManagerError,
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
}
