/// Which of the three TMS topics this store tracks "pending" (new data not yet published/retained)
/// for. The two telemetry topics come from the latest `CanonicalReading`; the diagnostic topic
/// comes from the latest `CanonicalFault` — mirrors `lib_mapping::render::TmsTopic` plus the one
/// fault-sourced topic `render_tms` doesn't cover (`lib_mapping::render_fault`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingTopic {
    /// `tms::DcMeasurementUpdate`.
    DcMeasurementUpdate,
    /// `tms::PowerHardwareUpdate`.
    PowerHardwareUpdate,
    /// `tms::ActiveDiagnosticState`.
    ActiveDiagnosticState,
}
