pub mod dbus_layer;

use tokio::sync::mpsc;
use tracing::{info, Level};
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

    // Disable trace and debug messages for the zbus crate,
    // this prevent infinite loop from occurring while sending the log 
    // over D-Bus
    let filter = filter.add_directive("zbus=info".parse().unwrap());

    let fmt_layer = fmt::layer();
    let dbus_layer = DBusLoggingLayer::new(log_tx);

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(dbus_layer)
        .with(filter)
        .init();
}

pub fn level_to_u8(level: Level) -> u8 {
    match level {
        Level::ERROR => 0,
        Level::WARN => 1,
        Level::INFO => 2,
        Level::DEBUG => 3,
        Level::TRACE => 4,
    }
}
