/// Failure opening a host serial port. Unlike [`crate::transport::IoError`] (the runtime read/write surface), this
/// keeps the *detail* — the OS/tokio-serial error or the offending parameter — so a "no such
/// device" or a permissions problem is visible in the log/journal instead of being flattened.
#[derive(Debug)]
pub enum OpenError {
    /// A line parameter could not be represented (e.g. an unsupported data-bit count).
    InvalidParams(&'static str),
    /// The OS/serial layer refused to open the port (not found, busy, no permission, …).
    Io(tokio_serial::Error),
    /// The configured RS-485 direction-control mechanism (`DirectionMode::KernelRs485` or
    /// `DirectionMode::GpioDe`) could not be initialized — the kernel's `TIOCSRS485` ioctl
    /// returned `ENOTTY`/`EINVAL` (the UART/driver doesn't implement it), or the configured GPIO
    /// chip/line could not be requested. Carries a message naming the failure AND the fallback
    /// modes (`rts` always works in software; `kernel`/`gpio` are the two hardware-assisted
    /// alternatives to each other).
    Direction(String),
}
