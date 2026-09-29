use super::rs232::Rs232;
use super::rs485::Rs485;

/// Enum factory over the concrete host transports — no `Box<dyn>`. Delegates `SerialTransport`
/// to the active variant.
pub enum HostSerial {
    Rs232(Rs232),
    Rs485(Rs485),
    /// No underlying port: the configured serial device could not be opened (missing/busy/no
    /// permission) at startup or reconnect. Every I/O call fails with [`crate::transport::IoError::Timeout`] so the
    /// poll loop's existing dead-cycle/backoff/reconnect machinery drives recovery — see
    /// [`HostSerial::disconnected`].
    Disconnected,
}
