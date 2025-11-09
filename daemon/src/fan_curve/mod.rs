use std::fmt::Debug;

pub mod fan_mode;
pub mod linear_curve;
pub mod hysteresis_curve;
pub mod fan_curve_info;

pub trait FanCurve: Debug {
    // Add a point to the fan curve
    // Points are always specified as (temp, fan_speed)
    fn add_point(&mut self, point: (i32, u8));
    // Update a point of the fan curve
    // Points are always specified as (temp, fan_speed)
    fn update_point(&mut self, point: (i32, u8));
    // Delete a point to the fan curve
    // Points are always specified as (temp, fan_speed)
    fn remove_point(&mut self, temp: i32);

    // Return the number of points in the curve
    fn points_num(&self) -> usize;

    // Return the fan speed for the given temperature
    fn get_speed(&self, temp: i32) -> u8;
}

