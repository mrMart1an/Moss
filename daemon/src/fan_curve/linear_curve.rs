use std::collections::BTreeMap;

use crate::fan_curve::{CurvePoint, FanCurve, FanSpeed};

pub struct LinearCurve {
    points: BTreeMap<u32, FanSpeed>,
}

impl LinearCurve {
    pub fn new() -> LinearCurve {
        LinearCurve {
            points: BTreeMap::new(),
        }
    }
}

impl FanCurve for LinearCurve {
    fn get_speed(&self, temp: u32) -> super::FanSpeed {
        // Check if temperature is the map, in that case return
        // the corresponding fan speed
        if let Some(speed) = self.points.get(&temp) {
            return speed.clone();
        }

        // Find the 2 points of the temperature interval
        // Find the preceding element
        let preceding = self.points.range(..temp).next_back();

        // Find the succeeding element
        let succeeding = self.points.range(temp..).next();

        if let Some(pre) = preceding {
            if let Some(suc) = succeeding {
                return linear_interpolation(pre, suc, temp);

                // If only one element was in the map return it
            } else {
                return pre.1.clone();
            }
        } else {
            if let Some(suc) = succeeding {
                return suc.1.clone();
            }
        }

        // If no element was in the map return 100 for safety
        FanSpeed::new(100)
    }

    fn add_point(&mut self, point: CurvePoint) {
        self.points.insert(point.temp, point.fan_speed);
    }

    fn remove_point(&mut self, point: CurvePoint) {
        self.points.remove(&point.temp);
    }

    fn points_num(&self) -> usize {
        self.points.len()
    }
}

// Perform the linear interpolation between 
// two points and return the fan speed
fn linear_interpolation(
    pre: (&u32, &FanSpeed),
    suc: (&u32, &FanSpeed),
    point: u32,
) -> FanSpeed {
    let p: f32 = point as f32;

    let x1: f32 = *pre.0 as f32;
    let y1: f32 = pre.1.get() as f32;
    let x2: f32 = *suc.0 as f32;
    let y2: f32 = suc.1.get() as f32;

    let m: f32 = (y1 - y2) / (x1 - x2);
    let b: f32 = (x1 * y2 - x2 * y1) / (x1 - x2);

    FanSpeed::new((m * p + b) as u32)
}
