use std::time::Duration;
use tokio_serial::SerialStream;

/// RS-232 transport over a tokio-serial port.
pub struct Rs232 {
    pub(in crate::host) stream: SerialStream,
    pub(in crate::host) read_timeout: Duration,
}
