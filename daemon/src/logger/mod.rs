pub mod dbus_layer;

use tokio::sync::mpsc;
use tracing::info;
use tracing_subscriber::{prelude::*, fmt, EnvFilter};

use crate::logger::dbus_layer::{DBusLog, DBusLoggingLayer};

pub fn init_logging(log_tx: mpsc::Sender<DBusLog>) {
    let level = if cfg!(debug_assertions) { "trace" } else { "info" };

    let filter = match EnvFilter::try_from_default_env() {
        Ok(f) => f,
        Err(_) => { 
            info!("\"RUST_LOG\" variable not set, defaulting to {level}");
            EnvFilter::new(level) 
        }
    };

    let fmt_layer = fmt::layer();
    let dbus_layer = DBusLoggingLayer::new(log_tx);

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(dbus_layer)
        .with(filter)
        .init();
}


