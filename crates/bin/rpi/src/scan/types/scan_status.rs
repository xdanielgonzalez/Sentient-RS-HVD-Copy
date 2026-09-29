/// One DIP address's scan outcome — a bus-walk table row's STATUS column.
#[derive(Debug, Clone, PartialEq)]
pub enum ScanStatus {
    /// A device answered at this address and its telemetry frame decoded cleanly.
    Live,
    /// No response within the read timeout — the expected outcome for an unpopulated DIP slot.
    Silent,
    /// Something answered but the exchange failed: bad checksum, a wrong echoed address, a
    /// collision, or a short/garbled frame. Carries a short diagnostic detail for the table.
    Error(String),
}
