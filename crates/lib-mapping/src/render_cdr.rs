//! Serialize-on-demand entry point for the DDS transport: canonical reading/fault -> TMS **XCDR2**
//! (DDS-XTypes 1.3, D_CDR2_LE), in one call, into a caller-owned buffer. The CDR twin of
//! [`crate::render`] (which renders TMS JSON for the MQTT transport).
//!
//! Every function here emits the FULL RTPS SerializedPayload: the 4-byte encapsulation header
//! (`D_CDR2_LE` + options, the options' trailing-pad count set at finalize) + the top-level DHEADER
//! + the members, per [`lib_tms::cdr`]'s spec-pinned core. Values are UNSCALED (real `f32`/integer
//! types) — the canonical scaled integers are converted to their real quantities in the CDR view
//! bindings (`wire::generated_cdr_encoders` for the battery subset, `wire::inverter_cdr` /
//! `wire::storage_cdr` hand-written for inverter/storage), exactly mirroring the JSON `render_*`
//! dispatch.
//!
//! **Counting/write parity.** [`write_cdr`] drives a [`CdrWriteSink`]; [`counted_len_cdr`] drives a
//! [`CdrCountingSink`] over the *same* chain and adds the same trailing encapsulation padding, so a
//! pre-sized length can never desync from the emitted byte count (`wire_cdr_len_matches_emit`
//! proptest).

use crate::codec::CodecError;
use crate::render::TmsTopic;
use crate::wire::{
    ActiveDiagnosticStateCdrView, ActiveDiagnosticStateInverterCdrView, DcMeasurementUpdateCdrView,
    DcMeasurementUpdateInverterCdrView, PowerHardwareUpdateCdrView,
    PowerHardwareUpdateInverterCdrView, StorageUpdateCdrView,
};
use lib_parse::canonical::{
    CanonicalFault, CanonicalInverterFault, CanonicalInverterReading, CanonicalReading,
    InverterMode,
};
use lib_tms::cdr::{emit_object_cdr, CdrChain, CdrCountingSink, CdrWriteSink, HasCdrChain};
use lib_tms::{ClockMonotonic, Identity};

/// Serialize `src` as a complete RTPS XCDR2 SerializedPayload driven by member chain `C`: the
/// encapsulation header + DHEADER + members ([`emit_object_cdr`]), then the trailing 4-byte-boundary
/// padding ([`CdrWriteSink::finalize_encapsulation_padding`]). Returns the total byte count, or
/// [`CodecError::BufferTooSmall`] if `out` overflowed (never a truncated payload — the write sink
/// latches overflow). The `chains == reference` byte-oracle asserts against this.
pub fn write_cdr<Src: ?Sized, C: CdrChain<Src>>(
    src: &Src,
    out: &mut [u8],
) -> Result<usize, CodecError> {
    let mut sink = CdrWriteSink::new(out);
    emit_object_cdr::<Src, C, _>(src, &mut sink);
    sink.finalize_encapsulation_padding();
    if sink.overflowed() {
        Err(CodecError::BufferTooSmall)
    } else {
        Ok(sink.len())
    }
}

/// The byte count [`write_cdr`] would emit for `src` — the encapsulation header + DHEADER + members
/// counted through a [`CdrCountingSink`], plus the identical trailing 4-byte encapsulation padding.
/// Must equal [`write_cdr`]'s returned length for every input (the shared-traversal safety invariant;
/// `wire_cdr_len_matches_emit`).
pub fn counted_len_cdr<Src: ?Sized, C: CdrChain<Src>>(src: &Src) -> usize {
    let mut counter = CdrCountingSink::new();
    emit_object_cdr::<Src, C, _>(src, &mut counter);
    let count = counter.len();
    // Trailing encapsulation padding: pad the payload (everything after the 4-byte header) up to the
    // next 4-byte boundary, matching `finalize_encapsulation_padding`.
    let payload_len = count.saturating_sub(4);
    let pad = (4 - (payload_len % 4)) % 4;
    count + pad
}

/// Render a battery [`CanonicalReading`] as `topic`'s TMS XCDR2 payload into `out`. Mirrors
/// [`crate::render::render_tms`] (the JSON counterpart) for the two Seplos telemetry topics.
pub fn render_tms_cdr(
    topic: TmsTopic,
    device_id: Identity,
    reading: &CanonicalReading,
    now: ClockMonotonic,
    out: &mut [u8],
) -> Result<usize, CodecError> {
    match topic {
        TmsTopic::DcMeasurementUpdate => {
            let view = DcMeasurementUpdateCdrView {
                reading,
                device_id: &device_id,
                now: &now,
            };
            write_cdr::<_, lib_tms::generated_cdr::dc_measurement_update_cdr::CdrChain>(&view, out)
        }
        TmsTopic::PowerHardwareUpdate => {
            let view = PowerHardwareUpdateCdrView {
                reading,
                device_id: &device_id,
                now: &now,
            };
            write_cdr::<_, lib_tms::generated_cdr::power_hardware_update_cdr::CdrChain>(&view, out)
        }
    }
}

/// Render a battery [`CanonicalFault`] as `tms::ActiveDiagnosticState` XCDR2 into `out`. CDR twin of
/// [`crate::render::render_fault`].
pub fn render_fault_cdr(
    reading_fault: &CanonicalFault,
    device_id: Identity,
    now: ClockMonotonic,
    out: &mut [u8],
) -> Result<usize, CodecError> {
    let view = ActiveDiagnosticStateCdrView {
        fault: reading_fault,
        device_id: &device_id,
        now: &now,
    };
    write_cdr::<_, lib_tms::generated_cdr::active_diagnostic_state_cdr::CdrChain>(&view, out)
}

/// Render a battery [`CanonicalReading`] as `tms::StorageUpdate` XCDR2 into `out`. CDR twin of
/// [`crate::render::render_storage_update`].
pub fn render_storage_update_cdr(
    device_id: Identity,
    reading: &CanonicalReading,
    out: &mut [u8],
) -> Result<usize, CodecError> {
    let view = StorageUpdateCdrView {
        reading,
        device_id: &device_id,
    };
    write_cdr::<_, lib_tms::generated_cdr::storage_update_cdr::CdrChain>(&view, out)
}

/// Render inverter `reading` as `tms::AcMeasurementUpdate` XCDR2 into `out`. CDR twin of
/// [`crate::render::render_ac_measurement_inverter`].
pub fn render_ac_measurement_inverter_cdr(
    device_id: Identity,
    reading: &CanonicalInverterReading,
    now: ClockMonotonic,
    out: &mut [u8],
) -> Result<usize, CodecError> {
    let view = crate::wire::AcMeasurementUpdateInverterCdrView {
        reading,
        device_id: &device_id,
        now: &now,
    };
    write_cdr::<_, lib_tms::generated_cdr::ac_measurement_update_cdr::CdrChain>(&view, out)
}

/// Render inverter `reading` as `tms::DcMeasurementUpdate` XCDR2 into `out`. CDR twin of
/// [`crate::render::render_dc_measurement_inverter`].
pub fn render_dc_measurement_inverter_cdr(
    device_id: Identity,
    reading: &CanonicalInverterReading,
    now: ClockMonotonic,
    out: &mut [u8],
) -> Result<usize, CodecError> {
    let view = DcMeasurementUpdateInverterCdrView {
        reading,
        device_id: &device_id,
        now: &now,
    };
    write_cdr::<_, lib_tms::generated_cdr::dc_measurement_update_cdr::CdrChain>(&view, out)
}

/// Render inverter `reading` as `tms::PowerHardwareUpdate` XCDR2 into `out`. CDR twin of
/// [`crate::render::render_power_hardware_inverter`].
pub fn render_power_hardware_inverter_cdr(
    device_id: Identity,
    reading: &CanonicalInverterReading,
    now: ClockMonotonic,
    out: &mut [u8],
) -> Result<usize, CodecError> {
    let view = PowerHardwareUpdateInverterCdrView {
        reading,
        device_id: &device_id,
        now: &now,
    };
    write_cdr::<_, lib_tms::generated_cdr::power_hardware_update_cdr::CdrChain>(&view, out)
}

/// Render inverter `fault` + `mode` as `tms::ActiveDiagnosticState` XCDR2 into `out`. CDR twin of
/// [`crate::render::render_active_diagnostic_inverter`].
pub fn render_active_diagnostic_inverter_cdr(
    reading_fault: &CanonicalInverterFault,
    mode: InverterMode,
    device_id: Identity,
    now: ClockMonotonic,
    out: &mut [u8],
) -> Result<usize, CodecError> {
    let view = ActiveDiagnosticStateInverterCdrView {
        fault: reading_fault,
        mode,
        device_id: &device_id,
        now: &now,
    };
    write_cdr::<_, lib_tms::generated_cdr::active_diagnostic_state_cdr::CdrChain>(&view, out)
}

/// Generic identity-topic serializer: render a concrete `value` implementing [`HasCdrChain`] as its
/// full XCDR2 SerializedPayload into `out`. The CDR analogue of [`crate::render::render_value`] — the
/// entry point for the identity topics (`DeviceInfo`, `Heartbeat`, `Reply`, `PowerPortState`,
/// `EnergyStartStopState`, `DcLoadSharingState`, `ActiveMicrogridControllerState`, and the two
/// request types) whose concrete generated struct is serialized directly through its identity
/// `…CdrSrc` binding.
pub fn render_cdr_value<T: HasCdrChain>(value: &T, out: &mut [u8]) -> Result<usize, CodecError> {
    write_cdr::<T, T::CdrChain>(value, out)
}
