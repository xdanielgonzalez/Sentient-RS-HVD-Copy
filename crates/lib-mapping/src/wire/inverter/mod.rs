//! Hand-written wire bindings for inverter CONV-role telemetry topics (AC/DC measurement updates,
//! power hardware update, active diagnostics). These replace the materializing serde transforms
//! during the render-on-output redesign (WP5, WP6 — see `docs/wire-serialization.md` and
//! [`lib_mapping::render`] module doc comment).
//!
//! Each binding borrows canonical `CanonicalInverterReading`/`CanonicalInverterFault` + device
//! context (device_id, timestamp) and implements the generated field-chain `*Src` traits to emit
//! TMS JSON via the flat [`crate::wire`] serializer (zero-copy, constant-frame, iterative over
//! faults). Math is ported verbatim from [`crate::transform_inverter`] (scales, sign conventions,
//! NaN-required-field rules).

mod impl_ac_measurement;
mod impl_active_diagnostic;
mod impl_dc_measurement;
mod impl_power_hardware;

pub use impl_ac_measurement::AcMeasurementUpdateWire;
pub use impl_active_diagnostic::ActiveDiagnosticStateInverterWire;
pub use impl_dc_measurement::DcMeasurementUpdateInverterWire;
pub use impl_power_hardware::PowerHardwareUpdateInverterWire;
