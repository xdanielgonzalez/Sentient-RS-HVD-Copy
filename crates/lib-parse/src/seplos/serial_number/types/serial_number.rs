/// Maximum serial-number length retained (bounded, no heap).
pub const MAX_SERIAL_LEN: usize = 32;

/// A decoded device serial number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerialNumber(pub heapless::String<MAX_SERIAL_LEN>);
