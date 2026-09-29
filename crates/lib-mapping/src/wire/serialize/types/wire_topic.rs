/// Which TMS telemetry topic to render a [`crate::schema::CanonicalReading`] as (mirrors
/// [`crate::render::TmsTopic`] — kept as a separate type here since `render.rs`'s old-pipeline enum
/// is superseded by this module's callers in Phase 2, not merged, to keep this module independent of
/// `render.rs` during the Phase 1 host-test-only stage).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireTopic {
    /// `tms::DcMeasurementUpdate` (`topics::DC_MEASUREMENT_UPDATE`).
    DcMeasurementUpdate,
    /// `tms::PowerHardwareUpdate` (`topics::POWER_HARDWARE_UPDATE`).
    PowerHardwareUpdate,
}
