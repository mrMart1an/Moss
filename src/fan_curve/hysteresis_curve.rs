use std::{cell::RefCell, cmp::Ordering};

use crate::fan_curve::{CurvePoint, FanCurve, FanSpeed};

pub struct HysteresisCurve<T: FanCurve> {
    curve: T,

    // Store the temperature and fan speed of the last update
    last_update: RefCell<Option<(u32, FanSpeed)>>,

    // Hysteresis lower threshold
    // When the Delta-T is negative and greater than this
    // threshold an update will occur
    lower_threshold: u32,

    // Hysteresis upper threshold
    // When the Delta-T is positive and greater than this
    // threshold an update will occur
    upper_threshold: u32,
}

impl<T: FanCurve> HysteresisCurve<T> {
    pub fn new(curve: T, lower: u32, upper: u32) -> HysteresisCurve<T> {
        Self {
            curve,

            last_update: RefCell::new(None),

            lower_threshold: lower,
            upper_threshold: upper,
        }
    }

    fn update(&self, temp: u32) -> FanSpeed {
        let speed = self.curve.get_speed(temp);

        let mut last_update = self.last_update.borrow_mut();
        *last_update = Some((temp, speed));

        speed
    }
}

impl<T: FanCurve> FanCurve for HysteresisCurve<T> {
    fn get_speed(&self, temp: u32) -> FanSpeed {
        // If last update is None update immediately and return the result
        let last_update = self.last_update.borrow().clone();

        if let Some(last) = last_update {
            let last_temp = last.0;
            let delta: i32 = (temp as i32) - (last_temp as i32);

            let threshold = match delta.cmp(&0) {
                Ordering::Less => self.lower_threshold,
                Ordering::Equal => self.lower_threshold,
                Ordering::Greater => self.upper_threshold,
            };

            // If the Delta-T is greater or equal to the
            // threshold trigger an update
            if (delta.abs() as u32) >= threshold {
                self.update(temp)
            } else {
                last.1
            }
        } else {
            self.update(temp)
        }
    }

    fn add_point(&mut self, point: CurvePoint) {
        self.curve.add_point(point);
    }

    fn remove_point(&mut self, point: CurvePoint) {
        self.curve.remove_point(point);
    }

    fn points_num(&self) -> usize {
        self.curve.points_num()
    }
}
