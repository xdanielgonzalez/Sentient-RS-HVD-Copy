use super::scan_status::ScanStatus;

/// One DIP address's outcome from a bus walk ([`crate::scan::run`]) — a table row: the address
/// probed, the aggregate [`ScanStatus`], and the decoded pack voltage (millivolts) when the
/// address answered and its telemetry frame decoded cleanly.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanResult {
    /// The DIP address probed (`crate::scan::DIP_MIN..=crate::scan::DIP_MAX`, i.e. `0x00..=0x0F`).
    pub dip: u8,
    pub status: ScanStatus,
    /// Decoded pack (total) voltage in millivolts — `Some` only when `status` is
    /// [`ScanStatus::Live`].
    pub pack_mv: Option<u32>,
}
