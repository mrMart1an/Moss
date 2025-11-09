// Store the data required to create an hysteresis fan curve
#[derive(Debug, Default, Clone)]
pub struct FanCurveInfo {
    pub points: Vec<(i32, u8)>,

    pub lower_threshold: Option<u32>,
    pub upper_threshold: Option<u32>
}
