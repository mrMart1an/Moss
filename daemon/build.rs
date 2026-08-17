const SERVICE_NAME: Option<&str> = option_env!("SERVICE_NAME");

fn main() {
    // Check if the env are set
    if SERVICE_NAME.is_none() {
        panic!("SERVICE_NAME env not defined!");
    }
}
