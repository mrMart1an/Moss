use serde::{Deserialize, Serialize};

// Device fan mode
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum FanMode {
    Auto,
    Curve,

    Manual(u8),
}

