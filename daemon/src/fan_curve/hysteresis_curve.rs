use std::{cell::RefCell, cmp::Ordering};

use crate::fan_curve::{fan_curve_info::FanCurveInfo, linear_curve::LinearCurve, FanCurve};

#[derive(Debug)]
pub struct HysteresisCurve<T: FanCurve> {
    curve: T,

    // Store the temperature and fan speed of the last update
    last_update: RefCell<Option<(i32, u8)>>,

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
    // Create a hysteresis curve from an existing fan curve
    pub fn from_curve(
        curve: T,
        lower_threshold: u32,
        upper_threshold: u32,
    ) -> HysteresisCurve<T> {
        Self {
            curve,

            last_update: RefCell::new(None),

            lower_threshold,
            upper_threshold,
        }
    }

    // Create a hysteresis curve using a linear curve as the base
    pub fn new(
        points: &[(i32, u8)],
        lower_threshold: u32,
        upper_threshold: u32,
    ) -> HysteresisCurve<LinearCurve> {
        HysteresisCurve::<LinearCurve> {
            curve: LinearCurve::new(points),
            last_update: RefCell::new(None),
            lower_threshold,
            upper_threshold,
        }
    }

    // Create a hysteresis curve using a linear curve
    // as the base from the given info structure
    pub fn from_info(info: &FanCurveInfo) -> HysteresisCurve<LinearCurve> {
        HysteresisCurve::<LinearCurve> {
            curve: LinearCurve::new(&info.points),
            last_update: RefCell::new(None),
            lower_threshold: info.lower_threshold.unwrap_or(0),
            upper_threshold: info.upper_threshold.unwrap_or(0),
        }
    }

    fn update(&self, temp: i32) -> u8 {
        let speed = self.curve.get_speed(temp);

        let mut last_update = self.last_update.borrow_mut();
        *last_update = Some((temp, speed));

        speed
    }
}

impl<T: FanCurve> FanCurve for HysteresisCurve<T> {
    fn get_speed(&self, temp: i32) -> u8 {
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

    fn add_point(&mut self, point: (i32, u8)) {
        self.curve.add_point(point);
    }

    fn update_point(&mut self, point: (i32, u8)) {
        self.curve.add_point(point);
    }

    fn remove_point(&mut self, temp: i32) {
        self.curve.remove_point(temp);
    }

    fn points_num(&self) -> usize {
        self.curve.points_num()
    }
}
