/// Saturating, bounded health counters for the poll loop. Plain `u32`s — `no_std`, no heap.
///
/// Every counter saturates at `u32::MAX` rather than wrapping, so a long-running device never
/// silently resets its own history. [`healthy`](Health::healthy) is telemetry-success accounting
/// (used for the periodic log summary and other diagnostics) — it is **not** what the host platform
/// gates the systemd watchdog on; that's liveness (reaching `Platform::on_cycle` at all), decoupled
/// from telemetry health so a serial-degraded device stays up and self-heals instead of being
/// restarted by systemd (see bug #172).
///
/// ```rust
/// use spec_app::{Health, UNHEALTHY_AFTER_CYCLES};
///
/// let mut health = Health::default();
///
/// // A good cycle: telemetry decoded, payload accepted by the sink.
/// health.record_telemetry(true);
/// health.record_publish(true);
/// health.end_cycle(true);
/// assert_eq!(health.cycles, 1);
/// assert_eq!(health.telemetry_ok, 1);
/// assert!(health.healthy());
///
/// // Enough failed cycles back to back trip the threshold...
/// for _ in 0..UNHEALTHY_AFTER_CYCLES {
///     health.record_telemetry(false);
///     health.end_cycle(false);
/// }
/// assert_eq!(health.telemetry_err, 3);
/// assert!(!health.healthy());
///
/// // ...and a single successful cycle resets the streak.
/// health.record_telemetry(true);
/// health.end_cycle(true);
/// assert_eq!(health.consecutive_failed_cycles, 0);
/// assert!(health.healthy());
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Health {
    /// Completed poll cycles.
    pub cycles: u32,
    /// Telemetry exchanges that decoded to a reading.
    pub telemetry_ok: u32,
    /// Telemetry exchanges that failed (transport or parse).
    pub telemetry_err: u32,
    /// Fault exchanges that decoded.
    pub fault_ok: u32,
    /// Fault exchanges that failed.
    pub fault_err: u32,
    /// TMS payloads the sink accepted.
    pub publish_ok: u32,
    /// TMS payloads the sink rejected.
    pub publish_err: u32,
    /// Cycles in a row with zero successful exchanges (reset on any success).
    pub consecutive_failed_cycles: u32,
}
