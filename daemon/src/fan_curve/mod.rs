pub mod fan_mode;
pub mod linear_curve;
pub mod hysteresis_curve;

pub trait FanCurve {
    // Add a point to the fan curve
    fn add_point(&mut self, point: CurvePoint);
    // Delete a point to the fan curve
    fn remove_point(&mut self, point: CurvePoint);

    // Return the number of points in the curve
    fn points_num(&self) -> usize;

    // Return the fan speed for the given temperature
    fn get_speed(&self, temp: u32) -> FanSpeed;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FanSpeed {
    speed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurvePoint {
    pub temp: u32,
    pub fan_speed: FanSpeed,
}

impl FanSpeed {
    // Generate a new fan speed point
    // automatically clamp the given value between 0 and 100
    pub fn new(speed: u32) -> FanSpeed {
        FanSpeed { speed: speed.clamp(0, 100) }
    }

    // Return the stored fan speed
    pub fn get(&self) -> u32 {
        self.speed
    }
}

impl From<(u32, u32)> for CurvePoint {
    fn from(value: (u32, u32)) -> Self {
        Self {
            temp: value.0,
            fan_speed: FanSpeed::new(value.1),
        }
    }
}
