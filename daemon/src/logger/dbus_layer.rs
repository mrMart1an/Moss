use std::mem::take;

use tokio::sync::mpsc;
use tracing::{Level, Subscriber, field::Visit};
use tracing_subscriber::Layer;

pub struct DBusLog {
    level: Level,

    file: Option<String>,
    line: Option<u32>,

    message: String,
}

struct DBusVisitor {
    message: String,
}

// Layer to send logging information to the DBus manager
pub struct DBusLoggingLayer {
    sender: mpsc::Sender<DBusLog>,
}

impl DBusLoggingLayer {
    pub fn new(sender: mpsc::Sender<DBusLog>) -> Self {
        Self { sender }
    }
}

impl<S: Subscriber> Layer<S> for DBusLoggingLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        // Get the message
        let mut visitor = DBusVisitor::new();
        event.record(&mut visitor);

        let message = visitor.get_msg();

        // Get the file and line number
        let line = event.metadata().line();
        let file = if let Some(file_str) = event.metadata().file() {
            Some(file_str.to_string())
        } else {
            None
        };

        // Build the DBusLog struct
        let dbus_log = DBusLog {
            level: *event.metadata().level(),

            file,
            line,
            message,
        };

        self.sender.try_send(dbus_log).unwrap_or_else(|e| {
            println!("Error while sending log on DBus channel: {}", e);
        });
    }
}

impl Visit for DBusVisitor {
    fn record_debug(
        &mut self,
        field: &tracing::field::Field,
        value: &dyn std::fmt::Debug,
    ) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        }
    }
}

impl DBusVisitor {
    fn new() -> Self {
        DBusVisitor { message: String::new() }
    }

    // NOTE: This function destroy the string stored in the visitor
    fn get_msg(&mut self) -> String {
        // Move the string out of the visitor and return it
        // replacing it with an empty one
        let msg = take(&mut self.message);
        self.message = String::new();

        msg
    }
}
