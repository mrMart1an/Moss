const SERVICE_NAME: Option<&str> = option_env!("SERVICE_NAME");

pub fn service_name() -> &'static str {
    SERVICE_NAME.expect("SERVICE_NAME env var not set at compile time")
}
