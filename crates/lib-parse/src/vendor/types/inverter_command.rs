/// Vendor-neutral logical command a caller can request from any inverter.
///
/// Sibling of [`super::BmsCommand`] — inverters and BMS devices are different device classes
/// polled over different serial legs (RS-232 vs RS-485) with unrelated wire protocols, so this is
/// its own enum, not a reuse/extension of `BmsCommand`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InverterCommand {
    /// Read live status telemetry (grid/AC/battery/PV measurements). Voltronic: `QPIGS`.
    Status,
    /// Read the current operating mode. Voltronic: `QMOD`.
    Mode,
    /// Read protection/warning fault flags. Voltronic: `QPIWS`.
    Fault,
    /// Read static ratings (battery type, model variant, output wiring mode). Voltronic: `QPIRI`.
    Ratings,
    /// Read the device serial number. Voltronic: `QID`.
    SerialNumber,
}
