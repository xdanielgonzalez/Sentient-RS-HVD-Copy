use lib_mapping::schema::ClockMonotonic;
use lib_parse::canonical::{
    CanonicalFault, CanonicalInverterFault, CanonicalInverterReading, CanonicalReading,
};

/// A cross-cutting host event offered to every configured [`super::sink_set::SinkSet`] tap
/// alongside (not instead of) the primary MQTT publish — the seam a later work stream wires
/// `Platform::emit_reading`/`emit_fault` through once the disk/Greengrass/mesh secondary publish
/// targets land (see `docs/gap-analysis-meteor-parity.md`'s "Publisher fan-out with per-target
/// isolation" finding). Borrowed, not owned — no allocation on the hot poll-loop path just to
/// observe an event.
pub enum TapEvent<'a> {
    Reading {
        reading: &'a CanonicalReading,
        now: ClockMonotonic,
        bus: &'a str,
    },
    Fault {
        fault: &'a CanonicalFault,
        now: ClockMonotonic,
        bus: &'a str,
    },
    Published {
        topic: &'a str,
        payload: &'a [u8],
        bus: &'a str,
    },
    /// One decoded **inverter** composite reading (QPIGS + QMOD mode + QPIRI ratings) — the RS-232
    /// Voltronic leg's counterpart to [`TapEvent::Reading`] (D9/D10b). Forwarded to the Greengrass
    /// tap (which renders `lib_mapping::legacy::inverter_payload` → `telemetry/inverter/{id}`); the
    /// disk tap ignores it, and the mesh tap ignores it too (mesh inverter forwarding is out of
    /// scope, tracked #267).
    InverterReading {
        reading: &'a CanonicalInverterReading,
        now: ClockMonotonic,
        bus: &'a str,
    },
    /// One decoded **inverter** fault snapshot (QPIWS flags) — the counterpart to [`TapEvent::Fault`]
    /// (D9/D10b). The Greengrass tap updates its inverter fault-merge cache (the flags land in the
    /// SAME message as the next inverter reading); disk/mesh ignore it (mesh: #267).
    InverterFault {
        fault: &'a CanonicalInverterFault,
        now: ClockMonotonic,
        bus: &'a str,
    },
}
