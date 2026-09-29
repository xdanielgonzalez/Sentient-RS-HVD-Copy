use test_kit::seplos::{BatteryState, Behavior, DriftSpec, FaultFlags};

/// Control message parsed from one stdin line, applied to the running [`test_kit::seplos::SeplosSim`].
pub(crate) enum Control {
    Behavior(Behavior),
    Faults(FaultFlags),
    State(BatteryState),
    /// Enable (`Some`) or disable (`None`) time-modulated telemetry drift — see
    /// `impl_drift_spec.rs` in `test-kit`.
    Drift(Option<DriftSpec>),
}
