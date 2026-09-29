/// Seplos command codes (CID2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// `0x42` — telemetry (cell voltages, temps, current, SOC/SOH…).
    Telemetry,
    /// `0x44` — fault/warning bitfield.
    Fault,
    /// `0xA4` — device serial number.
    SerialNumber,
}
