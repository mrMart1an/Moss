// Device fan mode
#[derive(Debug, Clone, Copy)]
pub enum FanMode {
    Auto,
    Curve,

    Manual(u8),
}

