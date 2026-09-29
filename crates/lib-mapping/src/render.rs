//! Serialize-on-demand entry point: canonical reading/fault -> TMS JSON, in one call, into a
//! caller-owned buffer.
//!
//! This is the seam the memory-model redesign's render-on-output step calls through (see the
//! crate root docs' "MEMORY-MODEL REDESIGN" pointer): a caller holding only compact
//! `CanonicalReading`/`CanonicalFault` values (e.g. a broker rendering a pending publish on
//! demand — `lib_link::mqtt_server::BrokerCore::drain_outgoing`'s `render` closure) maps to the
//! TMS JSON for exactly the topic it needs, without ever materializing a TMS struct or more than
//! one JSON buffer at a time.
//!
//! Serialization is [`crate::wire`]'s flat, no-alloc, integer-formatting serializer — NOT
//! `crate::codec`'s serde/`ryu` path. `render_tms`/`render_fault` used to build the full TMS struct
//! (`PowerHardwareUpdate`'s 50-slot `EnergyStorageState`) then run it through `serde-json-core`'s
//! recursive `Serialize` dispatch, which reserved huge call frames on the broker thread
//! (`render_fault` 14,400 B) and was the ROOT CAUSE of a broker-thread stack overflow; the
//! `#[inline(never)]` this module carried was only a frame-placement mitigation, not a fix (see
//! `docs/wire-serialization.md`). `crate::wire`'s flat serializer removes the recursion entirely —
//! see that module's doc comment for the design and the before/after frame-size numbers.
//!
//! This module's public signature (topic enum, `(device_id, reading, now, out) ->
//! Result<usize, CodecError>`) is unchanged from the old serde-backed version, so every caller
//! (`bin_zephyr_platform::render_pending_topic`, `lib_mapping::sink::publish_tms`) needed no changes
//! for this swap.

use crate::codec::CodecError;
use crate::wire::{
    serialize_ac_measurement_inverter, serialize_active_diagnostic,
    serialize_active_diagnostic_inverter, serialize_dc_measurement,
    serialize_dc_measurement_inverter, serialize_into, serialize_power_hardware,
    serialize_power_hardware_inverter, serialize_storage_update,
};
use lib_parse::canonical::{
    CanonicalFault, CanonicalInverterFault, CanonicalInverterReading, CanonicalReading,
    InverterMode,
};
use lib_tms::{
    fields::{emit_object, HasChain},
    ClockMonotonic, Identity,
};

/// Which TMS telemetry topic to render a [`CanonicalReading`] as. Mirrors
/// [`lib_tms::topics`]'s `DC_MEASUREMENT_UPDATE`/`POWER_HARDWARE_UPDATE` (the two topics a
/// single Seplos telemetry frame maps to); the diagnostic topic (`ActiveDiagnosticState`) is
/// rendered from a [`CanonicalFault`] instead, via [`render_fault`], since it has no reading input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TmsTopic {
    /// `tms::DcMeasurementUpdate` (`topics::DC_MEASUREMENT_UPDATE`).
    DcMeasurementUpdate,
    /// `tms::PowerHardwareUpdate` (`topics::POWER_HARDWARE_UPDATE`).
    PowerHardwareUpdate,
}

/// Map `reading` to the TMS JSON for `topic` via [`crate::wire`]'s flat serializer, writing into
/// `out` and returning the number of bytes written.
///
/// `device_id`/`now` are threaded straight into the rendered JSON's `device_id`/timestamp field —
/// this function performs no I/O and touches no state beyond its arguments.
pub fn render_tms(
    topic: TmsTopic,
    device_id: Identity,
    reading: &CanonicalReading,
    now: ClockMonotonic,
    out: &mut [u8],
) -> Result<usize, CodecError> {
    let n = match topic {
        TmsTopic::DcMeasurementUpdate => serialize_into(out, |s| {
            serialize_dc_measurement(reading, &device_id, now, s)
        }),
        TmsTopic::PowerHardwareUpdate => serialize_into(out, |s| {
            serialize_power_hardware(reading, &device_id, now, s)
        }),
    };
    n.ok_or(CodecError::BufferTooSmall)
}

/// Map `reading_fault` to `tms::ActiveDiagnosticState` (`topics::ACTIVE_DIAGNOSTIC_STATE`) JSON via
/// [`crate::wire`]'s flat serializer, writing into `out` and returning the number of bytes written.
/// See [`render_tms`]'s doc comment — same shape, for the one topic sourced from a
/// [`CanonicalFault`] rather than a [`CanonicalReading`].
pub fn render_fault(
    reading_fault: &CanonicalFault,
    device_id: Identity,
    now: ClockMonotonic,
    out: &mut [u8],
) -> Result<usize, CodecError> {
    serialize_into(out, |s| {
        serialize_active_diagnostic(reading_fault, &device_id, now, s)
    })
    .ok_or(CodecError::BufferTooSmall)
}

/// Map inverter `reading` to `tms::ac::MeasurementUpdate` (`topics::AC_MEASUREMENT_UPDATE`) JSON
/// via [`crate::wire`]'s flat serializer, writing into `out` and returning the number of bytes
/// written. Inverter CONV-role telemetry; the battery counterpart (if any) has no AC output.
pub fn render_ac_measurement_inverter(
    device_id: Identity,
    reading: &CanonicalInverterReading,
    now: ClockMonotonic,
    out: &mut [u8],
) -> Result<usize, CodecError> {
    serialize_into(out, |s| {
        serialize_ac_measurement_inverter(reading, &device_id, now, s)
    })
    .ok_or(CodecError::BufferTooSmall)
}

/// Map inverter `reading` to `tms::dc::MeasurementUpdate` (`topics::DC_MEASUREMENT_UPDATE`) JSON
/// via [`crate::wire`]'s flat serializer (inverter-sourced PV + battery ports). This complements
/// the battery `render_tms(TmsTopic::DcMeasurementUpdate, ...)` path — when a device has both
/// battery and inverter legs, they render via different call sites to different topic names
/// (the inverter port numbers 2/3 and battery port 0 do not overlap).
pub fn render_dc_measurement_inverter(
    device_id: Identity,
    reading: &CanonicalInverterReading,
    now: ClockMonotonic,
    out: &mut [u8],
) -> Result<usize, CodecError> {
    serialize_into(out, |s| {
        serialize_dc_measurement_inverter(reading, &device_id, now, s)
    })
    .ok_or(CodecError::BufferTooSmall)
}

/// Map inverter `reading` to `tms::PowerHardwareUpdate` (`topics::POWER_HARDWARE_UPDATE`) JSON
/// via [`crate::wire`]'s flat serializer (inverter-sourced power_electronics.temperature only).
/// Complements the battery `render_tms(TmsTopic::PowerHardwareUpdate, ...)` path (which carries
/// energy_storage state); the two may coexist if a device has both legs.
pub fn render_power_hardware_inverter(
    device_id: Identity,
    reading: &CanonicalInverterReading,
    now: ClockMonotonic,
    out: &mut [u8],
) -> Result<usize, CodecError> {
    serialize_into(out, |s| {
        serialize_power_hardware_inverter(reading, &device_id, now, s)
    })
    .ok_or(CodecError::BufferTooSmall)
}

/// Map inverter `fault` + `mode` to `tms::ActiveDiagnosticState` (`topics::ACTIVE_DIAGNOSTIC_STATE`)
/// JSON via [`crate::wire`]'s flat serializer (QPIWS faults + synthesized QMOD fault DTC).
/// Complements the battery `render_fault` path (which carries Seplos fault flags only).
pub fn render_active_diagnostic_inverter(
    reading_fault: &CanonicalInverterFault,
    mode: InverterMode,
    device_id: Identity,
    now: ClockMonotonic,
    out: &mut [u8],
) -> Result<usize, CodecError> {
    serialize_into(out, |s| {
        serialize_active_diagnostic_inverter(reading_fault, mode, &device_id, now, s)
    })
    .ok_or(CodecError::BufferTooSmall)
}

/// Map `reading` to `tms::StorageUpdate` (`topics::STORAGE_UPDATE`) JSON via [`crate::wire`]'s
/// flat serializer, writing into `out` and returning the number of bytes written. Battery energy
/// storage state with nested cell array and temperature aggregations (min/avg/max).
pub fn render_storage_update(
    device_id: Identity,
    reading: &CanonicalReading,
    out: &mut [u8],
) -> Result<usize, CodecError> {
    serialize_into(out, |s| serialize_storage_update(reading, &device_id, s))
        .ok_or(CodecError::BufferTooSmall)
}

/// Generic identity-topic serializer: render an `value` implementing [`HasChain`] as JSON into
/// `out`, returning the number of bytes written. No allocation, zero-copy reads through `&value`.
/// Byte-identical to `crate::codec::to_json_slice` for the same value (both use the same `ryu`
/// float path and string escaping).
///
/// This is the WP7 seam for identity topics (DeviceInfo, Heartbeat, Reply, PowerPortState,
/// EnergyStartStopState, DcLoadSharingState, ActiveMicrogridControllerState) — deprecated call
/// sites (`heartbeat.rs:399`, `reply_listener.rs:40`, `util.rs:58`) replace serde's
/// `codec::to_json_slice` with this zero-copy chain-driven path.
pub fn render_value<T: HasChain>(value: &T, out: &mut [u8]) -> Result<usize, CodecError> {
    serialize_into(out, |sink| emit_object::<T, T::Chain, _>(value, sink))
        .ok_or(CodecError::BufferTooSmall)
}
