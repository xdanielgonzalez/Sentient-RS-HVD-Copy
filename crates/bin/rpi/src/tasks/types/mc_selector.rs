use std::time::Duration;

use lib_mapping::schema::Identity;

use super::mc_candidate::McCandidate;

/// The MC-selection state machine (MIL-STD-3071 A.7.5, pp.29-30): consumes microgrid-controller
/// Heartbeats + DeviceInfo and selects the active MC, applying the New-MC / Lost-Active-MC / No-MC
/// timers. Pure and host-testable — [`crate::tasks::mc_select::run_mc_select`] drives it from the
/// embedded broker; the timing/selection logic itself takes no I/O (see
/// `crate::tasks::impl_mc_selector`).
///
/// Bounded: at most [`McSelector::MAX_CANDIDATES`] distinct MC senders are tracked (a microgrid has
/// a handful of controllers; extra senders past the cap are ignored with a one-shot warning).
///
/// The four timing fields below are set at construction from a resolved
/// `bin_rpi_platform::TimingProfile` (production values by default — see
/// [`McSelector::new`]/[`McSelector::with_profile`]); TEST-ONLY compression (debug builds, via
/// `SENTIENT_TEST_FAST_TIMERS`) is the caller's concern (`crate::tasks::mc_select::run_mc_select`),
/// not this type's — `McSelector` itself just applies whatever durations it's given.
pub struct McSelector {
    /// Every sender we have heard a Heartbeat and/or DeviceInfo from (not all are MCs).
    pub(in super::super) candidates: heapless::Vec<McCandidate, 8>,
    /// The currently-selected active MC id (`None` = No MC).
    pub(in super::super) selected: Option<Identity>,
    /// Edge-trigger guard so the No-MC / CONFIG_ON_COMMS_LOSS condition logs once per entry into it.
    pub(in super::super) no_mc_logged: bool,
    /// Heartbeat staleness deadline (B.4.1: Heartbeat is Periodic 1 s, so 3 missed beats = not
    /// live, at production timings).
    pub(in super::super) heartbeat_deadline: Duration,
    /// New-MC dwell: a live MC must be continuously live this long before adoption (A.7.5).
    pub(in super::super) new_mc: Duration,
    /// Lost-Active-MC: the active MC is dropped once its Heartbeat is silent this long (A.7.5).
    pub(in super::super) lost_active_mc: Duration,
    /// No-MC: once no MC Heartbeat has been heard this long, enter the No-MC state (A.7.5).
    pub(in super::super) no_mc: Duration,
}
