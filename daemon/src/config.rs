const SERVICE_NAME: Option<&str> = option_env!("SERVICE_NAME");
const DEFAULT_CONFIG_PATH: Option<&str> =
    option_env!("DEFAULT_CONFIG_PATH");

pub fn service_name() -> &'static str {
    SERVICE_NAME.expect("SERVICE_NAME not set!")
}
pub fn default_config_path() -> &'static str {
    DEFAULT_CONFIG_PATH.expect("DEFAULT_CONFIG_PATH not set!")
}
