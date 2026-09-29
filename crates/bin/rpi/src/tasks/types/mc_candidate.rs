use std::time::Instant;

use lib_mapping::schema::Identity;

/// One observed microgrid-controller (MC) candidate tracked by
/// [`super::mc_selector::McSelector`]. A sender becomes a live MC once it has BOTH a `DeviceInfo`
/// declaring role `ROLE_MICROGRID_CONTROLLER` (`is_mc`) AND a fresh Heartbeat (`last_heartbeat`
/// within the deadline).
pub struct McCandidate {
    /// The MC's device id (from the `tms/{id}/...` topic segment).
    pub(in super::super) id: Identity,
    /// True once a `DeviceInfo` with role `ROLE_MICROGRID_CONTROLLER` was seen for this id.
    pub(in super::super) is_mc: bool,
    /// Most recent Heartbeat instant from this id (`None` until the first heartbeat is heard —
    /// e.g. a `DeviceInfo` seen before any Heartbeat).
    pub(in super::super) last_heartbeat: Option<Instant>,
    /// Start of the current continuous-Heartbeat streak (a gap exceeding the heartbeat deadline
    /// starts a new streak) — the New-MC dwell anchor. Maintained in `on_heartbeat` so the dwell
    /// accrues from the streak's first beat regardless of when `evaluate` runs; a lapse in beats
    /// resets it, so a flapping MC re-earns the full dwell.
    pub(in super::super) streak_start: Option<Instant>,
}
