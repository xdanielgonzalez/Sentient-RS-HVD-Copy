use serde::{Deserialize, Serialize};

use super::device_rail::DeviceRail;

/// One INA3221 channel reading, in raw device units. This is the **adapter board's own power
/// draw** on the W5500/Heltec rails — engineering self-telemetry for diagnostics + the on-device
/// UI ONLY. Novel canonical type (no MIL-STD-3071 analogue).
///
/// **This is NOT the project's power telemetry.** The mission's power source is the Seplos BMS
/// (pack voltage/current/SOC), mapped to TMS in `lib_mapping::transform.rs`. An earlier wave
/// mistakenly mapped this type onto `DcMeasurementUpdate.internal_measurement` — the same TMS
/// mission topic the pack uses — which would have presented our own adapter's rails to a TMS
/// consumer as if they were pack-adjacent mission data. That mapping was removed; this type has no
/// TMS mapper at all and must never gain one. See ADR 0012 for the corrected record.
///
/// Per lib-canonical's standing "raw units in, scaling is the mapper's concern" doctrine, this
/// carries exactly what the INA3221 register map reports (shunt-voltage drop and bus voltage), not
/// derived engineering units — no amps, no watts, no shunt-resistance division. That scaling
/// happens at the diagnostic-publish call site (`bin-mcu-platform`), which renders a small,
/// non-TMS JSON payload onto a `diag/rails` topic.
///
/// # Example
///
/// ```
/// use lib_canonical::{DeviceRail, DeviceRailReading};
///
/// // ~150 mA on a 0.05 Ω shunt reads back as a 7500 µV drop; 5.05 V bus.
/// let reading = DeviceRailReading {
///     rail: DeviceRail::Ethernet,
///     shunt_uv: 7_500,
///     bus_mv: 5_050,
/// };
///
/// assert_eq!(reading.rail, DeviceRail::Ethernet);
/// assert_eq!(reading.bus_mv, 5_050);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRailReading {
    /// Which rail this channel monitors.
    pub rail: DeviceRail,
    /// Shunt voltage drop, in microvolts. Signed because the INA3221's shunt-voltage register is a
    /// signed two's-complement field on the wire (a reversed current direction would read
    /// negative), even though every rail here is expected to be a one-directional load.
    pub shunt_uv: i32,
    /// Bus voltage, in millivolts. Unsigned — the INA3221 bus-voltage register cannot go negative.
    pub bus_mv: u16,
}
