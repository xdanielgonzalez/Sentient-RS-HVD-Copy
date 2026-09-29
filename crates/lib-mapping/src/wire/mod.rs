//! Flat, `no_std`, no-alloc, integer-formatting `Canonical -> TMS-JSON` serializer — the ROOT fix
//! for the broker-thread stack overflow `render.rs`'s old `#[inline(never)]` band-aid only
//! mitigated (see `docs/wire-serialization.md` for the full design rationale + before/after
//! numbers).
//!
//! The old path built a full TMS struct (`PowerHardwareUpdate`'s 50-slot `EnergyStorageState`) then
//! ran it through `serde-json-core`'s **recursive** `Serialize` dispatch down to `ryu::format32` —
//! one call frame per nesting level, `render_fault` alone reserving 14,400 B. This module instead
//! walks each `CanonicalReading`/`CanonicalFault` with a single flat `for` loop and writes TMS JSON
//! directly through a [`JsonSink`] — no serde, no intermediate TMS struct, no `f32` in THIS module's
//! own encoders (`JsonSink::f32`/`f64` exist on the trait, via `crate::wire::float_digits`'s `ryu`
//! helpers, for genuinely-float fields elsewhere; the `serialize_*` functions below never call
//! them). Canonical values are already scaled integers (centi-volts, signed centi-amps, millivolts,
//! deci-percent, raw Kelvin×10); [`JsonSink::fixed`] formats them as decimals by placing the
//! decimal point positionally (`5452` at 2 fraction digits -> `"54.52"`), never converting to
//! float. JSON structure/field-name literals are `&'static str` — compile-time constants in
//! `.rodata`/flash, zero RAM cost.
//!
//! **One shared traversal for length + bytes.** [`serialized_len`] runs a serialize function
//! against a [`CountingSink`] (tallies bytes, emits nothing); [`serialize_into`] runs the *same*
//! serialize function against a [`WriteSink`] (emits the real bytes into a caller buffer). Because
//! both drive the identical call sequence, the counted length can never desync from the emitted
//! byte count — the exact property `docs/wire-serialization.md` calls out as the safety invariant
//! (an MQTT PUBLISH length prefix that disagrees with the body corrupts the packet), and the
//! `counted_len_matches_emitted_len` proptest in `tests/wire_len_matches_emit.rs` is the proof.
//!
//! Output JSON shape/field names/values match the existing `ToTms`/`ToDiagnostics` +
//! `codec::to_json_slice` (serde) pipeline (see `tests/wire_value_correctness.rs`), with one
//! documented difference: fixed-precision decimals, not `ryu`'s shortest-round-trip float
//! formatting (e.g. `54.5` may render `54.50`) — always valid JSON, within the TMS accuracy
//! tolerance (±0.005, `docs/tms-compliance.md`). `run_time`/`power_rate_of_change` (always `NaN` in
//! the old pipeline, since the Seplos source has no data for either) emit the JSON `null` literal
//! directly via [`JsonSink::null`], matching `serde_json_core`'s own `NaN`-has-no-JSON-literal
//! behavior for those two fields (this module does not carry a `NaN` representation anywhere — it
//! never touches `f32`).
//!
//! `forbid(unsafe_code)` (crate-level, `lib.rs`) — every write here is a checked slice/array
//! access, matching the rest of `lib-mapping`.

/// The `@generated` battery canonical->CDR (XCDR2) view bindings — the CDR twin of
/// `generated_encoders` (emitted by `tools/tms-idl-codegen`, since it needs both the `…CdrSrc`
/// traits and the canonical model). `render_cdr` builds these and drives `lib_tms::cdr`'s
/// `emit_object_cdr`. Inverter/storage CDR bindings are the hand-written `inverter_cdr`/`storage_cdr`
/// siblings.
mod generated_cdr_encoders;
/// The `@generated` flat canonical->TMS-wire encoders — the bridge half of the `lib-tms`/
/// `lib-canonical` split (emitted by `tools/tms-idl-codegen` into this crate, since it needs both
/// `JsonSink` and the canonical model). Byte-identical to the historical hand-written serializers;
/// the public `serialize_*` fns below delegate to it.
mod generated_encoders;
// `ryu`-based `f32`/`f64` formatting shared by `impl_counting_sink`/`impl_write_sink` — the digit
// math for `raw_int`/`fixed` moved to `lib_tms::fields::fixed_digits` (the shared-traversal
// invariant doc comment above now spans two crates for that reason); this stays local because
// `ryu` must never enter `lib-tms`'s no_std/no-alloc leaf dependency surface.
mod float_digits;
mod impl_counting_sink;
mod impl_write_sink;
pub mod inverter;
pub mod inverter_cdr;
mod serialize;
pub mod storage;
pub mod storage_cdr;
mod types;

pub use generated_cdr_encoders::{
    ActiveDiagnosticStateCdrView, DcMeasurementUpdateCdrView, PowerHardwareUpdateCdrView,
};
pub use generated_encoders::{
    ActiveDiagnosticStateWire, DcMeasurementUpdateWire, PowerHardwareUpdateWire,
};
pub use inverter::{
    AcMeasurementUpdateWire, ActiveDiagnosticStateInverterWire, DcMeasurementUpdateInverterWire,
    PowerHardwareUpdateInverterWire,
};
pub use inverter_cdr::{
    AcMeasurementUpdateInverterCdrView, ActiveDiagnosticStateInverterCdrView,
    DcMeasurementUpdateInverterCdrView, PowerHardwareUpdateInverterCdrView,
};
pub use serialize::{
    serialize_ac_measurement_inverter, serialize_active_diagnostic,
    serialize_active_diagnostic_inverter, serialize_dc_measurement,
    serialize_dc_measurement_inverter, serialize_into, serialize_power_hardware,
    serialize_power_hardware_inverter, serialize_storage_update, serialized_len, WireTopic,
};
pub use storage::StorageUpdateWire;
pub use storage_cdr::StorageUpdateCdrView;
pub use types::{CountingSink, JsonSink, WireError, WireFlat, WriteSink};
