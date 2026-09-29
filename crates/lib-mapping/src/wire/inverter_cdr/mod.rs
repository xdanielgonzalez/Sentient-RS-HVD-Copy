//! Hand-written XCDR2 CDR view bindings for inverter CONV-role telemetry topics — the CDR twin of
//! `wire::inverter`. Each view borrows a canonical inverter reading/fault plus device context and
//! implements the generated `…CdrSrc` accessor traits with UNSCALED values (real `f32`); the value
//! math is ported verbatim from `wire::inverter` (scales, sign conventions, NaN-required rules).
//! `render_cdr` builds these and drives `lib_tms::cdr::emit_object_cdr`.
//!
//! Generated only for the BATTERY subset (in `generated_cdr_encoders.rs`); inverter + storage CDR
//! bindings are hand-written, exactly as the JSON side splits generated battery bindings from
//! hand-written inverter/storage ones.

mod impl_ac_measurement;
mod impl_active_diagnostic;
mod impl_dc_measurement;
mod impl_power_hardware;

pub use impl_ac_measurement::AcMeasurementUpdateInverterCdrView;
pub use impl_active_diagnostic::ActiveDiagnosticStateInverterCdrView;
pub use impl_dc_measurement::DcMeasurementUpdateInverterCdrView;
pub use impl_power_hardware::PowerHardwareUpdateInverterCdrView;
