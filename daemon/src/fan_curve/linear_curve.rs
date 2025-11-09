use std::collections::BTreeMap;

use crate::fan_curve::FanCurve;

#[derive(Debug)]
pub struct LinearCurve {
    points: BTreeMap<i32, u8>,
}

impl LinearCurve {
    pub fn new(points: &[(i32, u8)]) -> LinearCurve {
        let mut curve = Self {
            points: BTreeMap::new(),
        };

        // Add the provided points to the curve
        for p in points {
            curve.add_point(p.clone());
        }

        curve
    }
}

impl FanCurve for LinearCurve {
    fn get_speed(&self, temp: i32) -> u8 {
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
                return pre.1.clone().clamp(0, 100);
            }
        } else {
            if let Some(suc) = succeeding {
                return suc.1.clone().clamp(0, 100);
            }
        }

        // If no element was in the map return 100 for safety
        100
    }

    fn add_point(&mut self, point: (i32, u8)) {
        // Clamp the fan speed
        self.points.insert(point.0, point.1.clamp(0, 100));
    }

    fn update_point(&mut self, point: (i32, u8)) {
        self.points.insert(point.0, point.1.clamp(0, 100));
    }

    fn remove_point(&mut self, temp: i32) {
        self.points.remove(&temp);
    }

    fn points_num(&self) -> usize {
        self.points.len()
    }
}

// Perform the linear interpolation between 
// two points and return the fan speed
fn linear_interpolation(
    pre: (&i32, &u8),
    suc: (&i32, &u8),
    temp: i32,
) -> u8 {
    let p: f32 = temp as f32;

    let x1: f32 = *pre.0 as f32;
    let y1: f32 = *pre.1 as f32;
    let x2: f32 = *suc.0 as f32;
    let y2: f32 = *suc.1 as f32;

    let m: f32 = (y1 - y2) / (x1 - x2);
    let b: f32 = (x1 * y2 - x2 * y1) / (x1 - x2);

    (m * p + b) as u8
}
