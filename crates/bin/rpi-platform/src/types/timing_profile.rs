use std::time::Duration;

/// The spec-mandated MIL-STD-3071 timers this adapter observes, gathered into one profile so a
/// single resolution point can (in non-shipping builds only) compress them for fast test runs.
///
/// **TEST-ONLY override — production behavior is unchanged.** [`TimingProfile::resolve`] returns
/// [`TimingProfile::production`] (the real spec values) in a release build, ALWAYS — there is no
/// code path in a release binary that can select [`TimingProfile::fast`]. In a debug build only,
/// setting the `SENTIENT_TEST_FAST_TIMERS` environment variable (to any value) switches
/// `resolve()` to the compressed [`TimingProfile::fast`] profile, so binary-spawning e2e tests
/// (`crates/test-e2e`) don't have to wait through the full multi-second spec windows in real
/// wall-clock time. The env read itself is behind `#[cfg(debug_assertions)]` — the shipped Pi
/// release binary cannot compress its timers even if the variable is (accidentally or otherwise)
/// present in its environment, so the production adapter is spec-conformant *by construction*, not
/// by convention.
///
/// Every field is a [`Duration`] since these gate real elapsed time (heartbeat staleness, MC
/// dwell/loss windows, the rate-limited-topic minimum interval, and the periodic re-evaluation
/// tick) — see `crate::tasks::impl_mc_selector` (bin-rpi) and
/// [`crate::EmitCadence`]/[`crate::HostPlatform`] (this crate) for how each field is
/// consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimingProfile {
    /// Heartbeat staleness deadline (MIL-STD-3071 B.4.1: Heartbeat is Periodic 1 s, so 3 missed
    /// beats before a candidate is considered not-live).
    pub heartbeat_deadline: Duration,
    /// New-MC dwell: a live MC must be continuously live this long before adoption as master
    /// (A.7.5).
    pub new_mc: Duration,
    /// Lost-Active-MC: the active MC is dropped once its Heartbeat is silent this long (A.7.5).
    pub lost_active_mc: Duration,
    /// No-MC: once no MC Heartbeat has been heard this long, enter the No-MC state (A.7.5).
    pub no_mc: Duration,
    /// Minimum interval between `PowerHardwareUpdate`/`StorageUpdate` publishes — spec Slow /
    /// Periodic 10 s with a "shall not send faster" rate limit (IDL 216-234; §4.9.10.2).
    pub rate_limit_period: Duration,
    /// Periodic re-evaluation cadence for the MC-selection tick, so its timers fire on elapsed
    /// time even with no inbound Heartbeat/DeviceInfo traffic.
    pub eval_tick: Duration,
}
