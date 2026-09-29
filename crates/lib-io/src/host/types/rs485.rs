use std::time::Duration;

use gpiocdev::Request;
use tokio_serial::SerialStream;

use super::direction_mode::DirectionMode;

/// RS-485 transport (half-duplex) over a tokio-serial port, with the RS-485 direction-control seam
/// (see [`DirectionMode`]): software RTS toggling (default), the kernel's native `TIOCSRS485`, or
/// a dedicated GPIO DE/RE line.
pub struct Rs485 {
    pub(in crate::host) stream: SerialStream,
    pub(in crate::host) read_timeout: Duration,
    /// This port's configured direction-control mechanism, set once at open (see
    /// `HostSerial::open_with_direction`) and read (never mutated) on every `set_direction` call.
    pub(in crate::host) direction: DirectionMode,
    /// The open GPIO line request for `DirectionMode::GpioDe` — held for the lifetime of the port
    /// (dropping it releases the line back to the kernel's default/tristate). `None` for every
    /// other direction mode.
    pub(in crate::host) gpio_line: Option<Request>,
}
