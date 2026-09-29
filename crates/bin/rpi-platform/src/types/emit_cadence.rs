use std::time::Duration;

use lib_parse::canonical::{CanonicalFault, CanonicalInverterFault, InverterMode};

/// Per-bus output-cadence state carried on [`super::host_platform::HostPlatform`], enforcing the
/// MIL-STD-3071 publication-timing profiles the raw poll loop (which fires every `interval_ms`)
/// would otherwise violate:
///
/// * **Rate limiting** — `PowerHardwareUpdate` and `StorageUpdate` are spec Slow / Periodic 10 s
///   with a "shall not send faster" rate limit (IDL 216-234; MIL-STD-3071 §4.9.10.2);
///   `DcMeasurementUpdate` is Periodic 1 s and is NOT gated. [`EmitCadence::rate_allow`] holds the
///   two 10 s topics to at-most-once-per-10 s while letting `DcMeasurementUpdate` through every
///   cycle. The secondary-publish taps (disk/mesh/Greengrass legacy) are also unaffected — they key
///   off the per-cycle `TapEvent::Reading`, not the rate-limited broker publishes.
/// * **ActiveDiagnosticState on-change** — spec At Start + On Change + PublishLast (B.12
///   pp.181-182). The poll loop reads faults on a fixed `fault_every` cadence; the `*_ads_due`
///   gates publish the first fault snapshot (at-start) and thereafter only when the fault set
///   changes, with a periodic safety-net re-publish (a documented PublishLast / durability
///   emulation — the in-process broker publisher cannot retain; see
///   `bin_rpi::tasks::heartbeat`'s DeviceInfo note and `lib_mqtt::broker`).
///
/// All timestamps are [`Duration`]s since the platform's monotonic origin (derived from the
/// [`lib_mapping::schema::ClockMonotonic`] the poll loop already passes into the emit seam), so every gate is
/// deterministic under test and independent of wall-clock.
pub struct EmitCadence {
    /// Last-emit instant (since origin) per rate-limited topic. Bounded — only the two 10 s topics
    /// (`PowerHardwareUpdate`, `StorageUpdate`) are ever inserted.
    pub(in super::super) rate_last: heapless::Vec<(&'static str, Duration), 2>,
    /// Last-published Seplos fault set (for the on-change gate) and when it published (for the
    /// safety-net gate).
    pub(in super::super) seplos_fault: Option<CanonicalFault>,
    pub(in super::super) seplos_emit: Option<Duration>,
    /// Last-published inverter fault set + QMOD mode, and when. `mode` participates because the
    /// QMOD mode-fault DTC is synthesized into the SAME `ActiveDiagnosticState` (D10), so a mode
    /// transition is itself an on-change event.
    pub(in super::super) inverter_fault: Option<(CanonicalInverterFault, InverterMode)>,
    pub(in super::super) inverter_emit: Option<Duration>,
}
