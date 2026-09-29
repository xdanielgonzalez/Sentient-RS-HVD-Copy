/// Vendor-neutral logical command a caller can request from any BMS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BmsCommand {
    /// Read live telemetry (cell voltages, temps, current, SOC/SOH…).
    Telemetry,
    /// Read protection/warning fault flags.
    Fault,
    /// Read the device serial number.
    SerialNumber,
}
