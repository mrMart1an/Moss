use crate::fan_curve::FanSpeed;

// Device fan mode
pub enum FanMode {
    Auto,
    Curve,

    Manual(FanSpeed),
}

