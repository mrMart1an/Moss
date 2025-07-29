use tracing::info;
use tracing_subscriber::{prelude::*, fmt, EnvFilter};

pub fn init_logging() {
    let level = if cfg!(debug_assertions) { "trace" } else { "info" };

    let filter = match EnvFilter::try_from_default_env() {
        Ok(f) => f,
        Err(_) => { 
            info!("\"RUST_LOG\" variable not set, defaulting to {level}");
            EnvFilter::new(level) 
        }
    };

    let fmt_layer = fmt::layer();

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(filter)
        .init();
}
