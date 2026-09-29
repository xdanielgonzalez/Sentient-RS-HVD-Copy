/// Transport error. Kept minimal and vendor/platform-neutral; concrete impls map their own
/// error into these.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IoError {
    /// Read/write timed out.
    Timeout,
    /// Underlying device/OS error.
    Device,
    /// Read returned fewer bytes than required / buffer full.
    ShortTransfer,
    /// Direction control failed (RS-485 driver-enable line).
    Direction,
}
