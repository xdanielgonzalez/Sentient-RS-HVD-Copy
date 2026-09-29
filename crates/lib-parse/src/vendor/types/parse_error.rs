/// Vendor-neutral parse error. Each vendor maps its wire-specific faults onto these.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Frame did not match the vendor's expected framing (start/end/length).
    BadFraming,
    /// Frame integrity check (checksum/CRC) failed.
    BadChecksum,
    /// Payload was too short or malformed for the declared layout.
    Malformed,
    /// Device reported an error return code.
    DeviceError(u8),
    /// Response address did not match the addressed device.
    AddressMismatch,
    /// Output buffer capacity exceeded (too many cells/temps/bytes).
    CapacityExceeded,
}
