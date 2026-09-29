use serde::{Deserialize, Serialize};

/// The rail one INA3221 channel is wired to — the adapter board's OWN W5500/Heltec rails,
/// engineering self-telemetry, NOT the project's power source (that is the Seplos BMS pack,
/// mapped in `lib_mapping::transform`). Novel canonical type (no MIL-STD-3071 analogue): the
/// standard has no concept of "which internal rail", only numbered internal ports — and this data
/// never reaches TMS at all (see [`super::device_rail_reading::DeviceRailReading`]'s doc comment
/// for the history: an earlier wave wrongly mapped this onto `DcMeasurementUpdate.
/// internal_measurement`, corrected — this is diagnostics/UI only). This enum is purely a
/// device-model/diagnostics concept.
///
/// The bench wires all three of the INA3221's channels: `Ethernet` (ch0, W5500), `Radio` (ch1,
/// Heltec), and `Mcu` (ch2, the RP2350 Feather's own supply). `Spare` remains for an
/// unpopulated/unmapped channel so the enum's shape does not need to change if a channel is later
/// left unwired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum DeviceRail {
    /// The W5500 Ethernet PHY rail.
    Ethernet,
    /// The Heltec V4 LoRa radio rail.
    Radio,
    /// The MCU's own power rail (RP2350 Feather supply) — INA3221 channel 2.
    Mcu,
    /// Unwired/unmapped INA3221 channel, reserved for a future rail.
    Spare,
}
