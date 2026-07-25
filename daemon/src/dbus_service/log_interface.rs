use zbus::{interface, object_server::SignalEmitter};

// Interface of the D-Bus service used for log reporting
#[derive(Default)]
pub struct LogInterface {}

#[interface(name = "com.github.Mossd1.Log")]
impl LogInterface {
    #[zbus(signal)]
    pub async fn new_log(
        emitter: &SignalEmitter<'_>,

        level: u8,

        file: String,
        line: i32,

        error: &str,
    ) -> zbus::Result<()>;
}
