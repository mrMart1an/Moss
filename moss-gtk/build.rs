const APP_ID: Option<&str> = option_env!("APP_ID");

fn main() {
    // Check if the env are set
    if APP_ID.is_none() {
        panic!("APP_ID env not defined!");
    }
}
