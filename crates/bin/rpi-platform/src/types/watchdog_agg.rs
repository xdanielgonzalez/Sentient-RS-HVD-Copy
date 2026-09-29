use super::cycle_pulse::CyclePulse;

/// Aggregates one [`CyclePulse`] per named background task (disk writer, mesh radio, location
/// refresher, ...) so a single host-side watchdog check can see whether every configured secondary
/// task is still alive — today `HostPlatform::on_cycle` only feeds the systemd watchdog on
/// reaching the primary poll loop's own cycle boundary.
#[derive(Clone, Default)]
pub struct WatchdogAgg {
    pub slots: Vec<(String, CyclePulse)>,
}
