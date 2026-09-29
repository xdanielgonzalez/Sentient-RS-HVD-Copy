//! The flat `Canonical -> TMS-JSON` serialize functions — see `mod.rs`'s module doc comment for the
//! design. Each is a single, non-recursive pass: straight-line field writes plus one `for` loop over
//! cells/DTCs, each iteration doing constant work — so the call frame is shallow and independent of
//! collection length (unlike the old serde path, whose frame depth tracked the TMS struct nesting).
//!
//! # Single source of truth: the generated encoders
//!
//! The encoder BODIES are no longer hand-written here — they are the `@generated`
//! [`crate::wire::DcMeasurementUpdateWire`]/… `WireFlat` impls (emitted by `tools/tms-idl-codegen`
//! from the IDL structure + `tms-overrides.toml` value bindings). These public
//! `serialize_dc_measurement`/… free functions are the stable seam every caller uses (`render.rs`,
//! the proofs); they simply build the per-topic wire binding and drive its `serialize_flat`. Keeping
//! ONE encoder implementation is exactly the anti-drift property the generator exists to provide —
//! the historical hand-written serializer these replace was PROVEN byte-identical to this generated
//! one (`tests/wire_byte_identity.rs`), and the independent serde/transform oracle
//! (`tests/wire_value_correctness.rs`) continues to pin the wire bytes.

mod types;

pub use types::*;

use lib_canonical::{
    CanonicalFault, CanonicalInverterFault, CanonicalInverterReading, CanonicalReading,
};
use lib_parse::canonical::InverterMode;
use lib_tms::{ClockMonotonic, Identity};

use super::{
    AcMeasurementUpdateWire, ActiveDiagnosticStateInverterWire, ActiveDiagnosticStateWire,
    DcMeasurementUpdateInverterWire, DcMeasurementUpdateWire, JsonSink,
    PowerHardwareUpdateInverterWire, PowerHardwareUpdateWire, StorageUpdateWire, WireFlat,
};

/// `tms::dc::MeasurementUpdate` (`topics::DC_MEASUREMENT_UPDATE`) — port 0, one external DC line,
/// sign-inverted amperage. Delegates to the generated [`DcMeasurementUpdateWire`] encoder (the
/// `serialize_flat` step is infallible — see [`crate::wire::WireError`]).
pub fn serialize_dc_measurement(
    reading: &CanonicalReading,
    device_id: &Identity,
    now: ClockMonotonic,
    sink: &mut impl JsonSink,
) {
    let _ = DcMeasurementUpdateWire {
        reading,
        device_id,
        now,
    }
    .serialize_flat(sink);
}

/// `tms::PowerHardwareUpdate` (`topics::POWER_HARDWARE_UPDATE`) carrying `EnergyStorageState`.
/// Delegates to the generated [`PowerHardwareUpdateWire`] encoder.
pub fn serialize_power_hardware(
    reading: &CanonicalReading,
    device_id: &Identity,
    now: ClockMonotonic,
    sink: &mut impl JsonSink,
) {
    let _ = PowerHardwareUpdateWire {
        reading,
        device_id,
        now,
    }
    .serialize_flat(sink);
}

/// `tms::ActiveDiagnosticState` (`topics::ACTIVE_DIAGNOSTIC_STATE`) — one DTC per set fault flag,
/// capped + overflow-counted. Delegates to the generated [`ActiveDiagnosticStateWire`] encoder.
pub fn serialize_active_diagnostic(
    fault: &CanonicalFault,
    device_id: &Identity,
    now: ClockMonotonic,
    sink: &mut impl JsonSink,
) {
    let _ = ActiveDiagnosticStateWire {
        fault,
        device_id,
        now,
    }
    .serialize_flat(sink);
}

/// Run `serialize` (one of [`serialize_dc_measurement`]/[`serialize_power_hardware`]/
/// [`serialize_active_diagnostic`]'s call shape) against a [`super::CountingSink`] and return the
/// byte length it would emit — see `mod.rs`'s "one shared traversal" doc comment.
pub fn serialized_len(serialize: impl FnOnce(&mut super::CountingSink)) -> usize {
    let mut counting = super::CountingSink::new();
    serialize(&mut counting);
    counting.len()
}

/// Run `serialize` against a [`super::WriteSink`] wrapping `out`, returning `Some(bytes_written)` on
/// success or `None` if `out` was too small (bounds-checked, never a panic/OOB — see
/// `types/write_sink.rs`'s doc comment).
pub fn serialize_into(
    out: &mut [u8],
    serialize: impl FnOnce(&mut super::WriteSink<'_>),
) -> Option<usize> {
    let mut writer = super::WriteSink::new(out);
    serialize(&mut writer);
    if writer.overflowed() {
        None
    } else {
        Some(writer.len())
    }
}

/// `tms::ac::MeasurementUpdate` (inverter sourced, `topics::AC_MEASUREMENT_UPDATE`). Delegates to
/// the hand-written [`AcMeasurementUpdateWire`] encoder.
pub fn serialize_ac_measurement_inverter(
    reading: &CanonicalInverterReading,
    device_id: &Identity,
    now: ClockMonotonic,
    sink: &mut impl JsonSink,
) {
    let _ = AcMeasurementUpdateWire {
        reading,
        device_id,
        now,
    }
    .serialize_flat(sink);
}

/// `tms::dc::MeasurementUpdate` (inverter sourced, `topics::DC_MEASUREMENT_UPDATE`). Delegates to
/// the hand-written [`DcMeasurementUpdateInverterWire`] encoder.
pub fn serialize_dc_measurement_inverter(
    reading: &CanonicalInverterReading,
    device_id: &Identity,
    now: ClockMonotonic,
    sink: &mut impl JsonSink,
) {
    let _ = DcMeasurementUpdateInverterWire {
        reading,
        device_id,
        now,
    }
    .serialize_flat(sink);
}

/// `tms::PowerHardwareUpdate` (inverter sourced, `topics::POWER_HARDWARE_UPDATE`). Delegates to
/// the hand-written [`PowerHardwareUpdateInverterWire`] encoder.
pub fn serialize_power_hardware_inverter(
    reading: &CanonicalInverterReading,
    device_id: &Identity,
    now: ClockMonotonic,
    sink: &mut impl JsonSink,
) {
    let _ = PowerHardwareUpdateInverterWire {
        reading,
        device_id,
        now,
    }
    .serialize_flat(sink);
}

/// `tms::ActiveDiagnosticState` (inverter sourced, `topics::ACTIVE_DIAGNOSTIC_STATE`) — QPIWS
/// fault flags + synthesized QMOD fault DTC. Delegates to the hand-written
/// [`ActiveDiagnosticStateInverterWire`] encoder.
pub fn serialize_active_diagnostic_inverter(
    fault: &CanonicalInverterFault,
    mode: InverterMode,
    device_id: &Identity,
    now: ClockMonotonic,
    sink: &mut impl JsonSink,
) {
    let _ = ActiveDiagnosticStateInverterWire {
        fault,
        mode,
        device_id,
        now,
    }
    .serialize_flat(sink);
}

/// `tms::StorageUpdate` (`topics::STORAGE_UPDATE`) — energy storage state with nested cell array
/// and temperature aggregations. Delegates to the hand-written [`StorageUpdateWire`] encoder.
pub fn serialize_storage_update(
    reading: &CanonicalReading,
    device_id: &Identity,
    sink: &mut impl JsonSink,
) {
    let _ = StorageUpdateWire { reading, device_id }.serialize_flat(sink);
}
