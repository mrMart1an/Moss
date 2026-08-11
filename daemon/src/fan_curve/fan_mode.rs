use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};

// Device fan mode
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum FanMode {
    Auto,
    Curve,

    Manual(u8),
}

impl From<FanMode> for String {
    fn from(value: FanMode) -> Self {
        match value {
            FanMode::Auto => "Auto".to_string(),
            FanMode::Curve => "Curve".to_string(),
            FanMode::Manual(speed) => format!("Manual:{}", speed),
        }
    }
}

impl TryFrom<String> for FanMode {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        // Match to lower case to allow for typos in the client services
        let value = value.to_lowercase();

        if value.contains("auto") {
            return Ok(FanMode::Auto);
        }
        if value.contains("curve") {
            return Ok(FanMode::Curve);
        }

        // Parse manual speed
        if value.contains("manual:") {
            if let Some((_, speed_str)) = value.split_once(":") {
                let speed: u8 = speed_str.parse::<u8>().with_context(
                    || "Failed to perform speed_str type conversion",
                )?;

                return Ok(FanMode::Manual(speed));
            }
        }

        Err(anyhow!("Failed to convert string to fan mode"))
    }
}
