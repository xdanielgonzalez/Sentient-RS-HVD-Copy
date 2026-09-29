use serde::{Deserialize, Serialize};

/// Vendor-neutral inverter operating mode. Novel canonical type (no MIL-STD-3071 analogue) — the
/// RS-232 inverter leg decodes its device-specific mode word into this common set. PROVISIONAL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum InverterMode {
    PowerOn,
    Standby,
    Line,
    Battery,
    Fault,
    PowerSaving,
    Unknown,
}
