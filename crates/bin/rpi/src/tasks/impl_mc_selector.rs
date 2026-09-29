use std::time::Instant;

use bin_rpi_platform::TimingProfile;
use lib_mapping::schema::Identity;

use crate::tasks::types::{McCandidate, McSelector};
use crate::tasks::util::bounded;

impl McSelector {
    /// Bounded candidate table capacity (mirrors the `heapless::Vec` bound on the type).
    pub const MAX_CANDIDATES: usize = 8;

    /// A fresh selector with no candidates and no MC selected, timed at the PRODUCTION spec
    /// values (`TimingProfile::production`) — this is what every existing unit test below (which
    /// injects explicit `now` offsets at the documented 3 s/6 s/10 s spec boundaries) exercises,
    /// so those tests keep passing unchanged regardless of the process's resolved
    /// `TimingProfile`. `crate::tasks::mc_select::run_mc_select` uses
    /// [`McSelector::with_profile`] instead, so the live task tracks whatever profile the process
    /// resolved at startup (production, or TEST-ONLY fast timings in a debug build).
    pub fn new() -> Self {
        Self::with_profile(&TimingProfile::production())
    }

    /// A fresh selector timed from `profile`'s four MC-selection durations (heartbeat deadline,
    /// New-MC dwell, Lost-Active-MC, No-MC) — see [`TimingProfile`] for the production-vs-fast
    /// contract. Used by `crate::tasks::mc_select::run_mc_select`, which resolves the profile once
    /// at process startup and passes it in here.
    pub fn with_profile(profile: &TimingProfile) -> Self {
        McSelector {
            candidates: heapless::Vec::new(),
            selected: None,
            no_mc_logged: false,
            heartbeat_deadline: profile.heartbeat_deadline,
            new_mc: profile.new_mc,
            lost_active_mc: profile.lost_active_mc,
            no_mc: profile.no_mc,
        }
    }

    /// Find (or, if room, create) the candidate entry for `id`. Returns `None` only when the table
    /// is full and `id` is new (the extra sender is ignored — logged once by the caller path).
    fn entry_mut(&mut self, id: &str) -> Option<&mut McCandidate> {
        if let Some(pos) = self.candidates.iter().position(|c| c.id.as_str() == id) {
            return self.candidates.get_mut(pos);
        }
        let candidate = McCandidate {
            id: bounded::<32>(id),
            is_mc: false,
            last_heartbeat: None,
            streak_start: None,
        };
        if self.candidates.push(candidate).is_err() {
            log::warn!(
                "MC-selection: candidate table full ({} entries); ignoring new sender '{id}'",
                Self::MAX_CANDIDATES
            );
            return None;
        }
        self.candidates.last_mut()
    }

    /// Record a Heartbeat from `id` at `now`. A gap longer than the heartbeat deadline since the
    /// previous beat starts a fresh streak (resetting the New-MC dwell anchor).
    pub fn on_heartbeat(&mut self, id: &str, now: Instant) {
        let deadline = self.heartbeat_deadline;
        if let Some(c) = self.entry_mut(id) {
            let new_streak = match c.last_heartbeat {
                Some(prev) => now.saturating_duration_since(prev) > deadline,
                None => true,
            };
            if new_streak {
                c.streak_start = Some(now);
            }
            c.last_heartbeat = Some(now);
        }
    }

    /// Record `id`'s observed role: `is_mc` = its `DeviceInfo` declared `ROLE_MICROGRID_CONTROLLER`.
    pub fn on_role(&mut self, id: &str, is_mc: bool) {
        if let Some(c) = self.entry_mut(id) {
            c.is_mc = is_mc;
        }
    }

    /// Whether `candidate` currently has a fresh (within-deadline) Heartbeat.
    fn is_live(&self, candidate: &McCandidate, now: Instant) -> bool {
        candidate.is_mc
            && candidate
                .last_heartbeat
                .is_some_and(|t| now.saturating_duration_since(t) <= self.heartbeat_deadline)
    }

    /// Apply the A.7.5 timers and return the current selection (`None` = No MC). Idempotent: call it
    /// on every inbound event and on a periodic tick; the returned value is the authoritative
    /// selection, and internal state is updated in place.
    pub fn evaluate(&mut self, now: Instant) -> Option<Identity> {
        // Keep the active MC while its Heartbeat is within the Lost-Active-MC window.
        if let Some(sel) = self.selected.clone() {
            let still_active = self.candidates.iter().any(|c| {
                c.id == sel
                    && c.is_mc
                    && c.last_heartbeat
                        .is_some_and(|t| now.saturating_duration_since(t) <= self.lost_active_mc)
            });
            if still_active {
                self.no_mc_logged = false;
                return self.selected.clone();
            }
            log::info!(
                "MC-selection: active MC '{sel}' lost (Heartbeat silent >{}s)",
                self.lost_active_mc.as_secs()
            );
            self.selected = None;
        }

        // No active MC: adopt the (deterministically smallest-id) live MC that has satisfied the
        // New-MC dwell.
        let mut chosen: Option<Identity> = None;
        for c in self.candidates.iter() {
            let dwelled = c
                .streak_start
                .is_some_and(|since| now.saturating_duration_since(since) >= self.new_mc);
            if dwelled && self.is_live(c, now) {
                let take = match &chosen {
                    Some(best) => c.id.as_str() < best.as_str(),
                    None => true,
                };
                if take {
                    chosen = Some(c.id.clone());
                }
            }
        }
        if let Some(id) = chosen {
            log::info!("MC-selection: selected MC '{id}' (New-MC dwell satisfied)");
            self.selected = Some(id.clone());
            self.no_mc_logged = false;
            return Some(id);
        }

        // No MC available. If none has been heard within No-MC, process CONFIG_ON_COMMS_LOSS.
        let any_recent_mc = self.candidates.iter().any(|c| {
            c.is_mc
                && c.last_heartbeat
                    .is_some_and(|t| now.saturating_duration_since(t) <= self.no_mc)
        });
        if !any_recent_mc && !self.no_mc_logged {
            log::info!(
                "MC-selection: No MC for >{}s — processing CONFIG_ON_COMMS_LOSS (documented no-op: \
                 no config subsystem)",
                self.no_mc.as_secs()
            );
            self.no_mc_logged = true;
        }
        None
    }
}

impl Default for McSelector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn selects_after_new_mc_dwell_not_before() {
        let mut s = McSelector::new();
        let start = t0();
        // MC announces itself (DeviceInfo role=MC) and starts beating.
        s.on_role("MC-1", true);
        s.on_heartbeat("MC-1", start);
        // Immediately: not yet adopted (New-MC dwell = 3 s).
        assert_eq!(s.evaluate(start), None);
        // Keep beating; still inside the dwell at +2 s.
        s.on_heartbeat("MC-1", start + Duration::from_secs(2));
        assert_eq!(s.evaluate(start + Duration::from_secs(2)), None);
        // At +3 s of continuous liveness: adopted.
        s.on_heartbeat("MC-1", start + Duration::from_secs(3));
        assert_eq!(
            s.evaluate(start + Duration::from_secs(3)).as_deref(),
            Some("MC-1")
        );
    }

    #[test]
    fn drops_active_mc_after_lost_window_then_no_mc() {
        let mut s = McSelector::new();
        let start = t0();
        s.on_role("MC-1", true);
        for k in 0..=3 {
            s.on_heartbeat("MC-1", start + Duration::from_secs(k));
        }
        assert_eq!(
            s.evaluate(start + Duration::from_secs(3)).as_deref(),
            Some("MC-1")
        );
        // Heartbeats stop. Still selected at +8 s (within Lost-Active 6 s of the last beat at +3).
        assert_eq!(
            s.evaluate(start + Duration::from_secs(8)).as_deref(),
            Some("MC-1")
        );
        // At +10 s (>6 s since last beat): active MC lost -> No MC.
        assert_eq!(s.evaluate(start + Duration::from_secs(10)), None);
    }

    #[test]
    fn ignores_non_mc_role_senders() {
        let mut s = McSelector::new();
        let start = t0();
        // A STOR device beats but never declares MC role.
        s.on_role("BATT-1", false);
        for k in 0..=4 {
            s.on_heartbeat("BATT-1", start + Duration::from_secs(k));
        }
        assert_eq!(s.evaluate(start + Duration::from_secs(4)), None);
    }

    #[test]
    fn device_info_before_heartbeat_needs_a_beat_to_go_live() {
        let mut s = McSelector::new();
        let start = t0();
        // DeviceInfo arrives, but no Heartbeat yet: not live, cannot be adopted.
        s.on_role("MC-1", true);
        assert_eq!(s.evaluate(start + Duration::from_secs(5)), None);
        // First beat starts the dwell; adopted 3 s later.
        s.on_heartbeat("MC-1", start + Duration::from_secs(5));
        s.on_heartbeat("MC-1", start + Duration::from_secs(8));
        assert_eq!(
            s.evaluate(start + Duration::from_secs(8)).as_deref(),
            Some("MC-1")
        );
    }

    #[test]
    fn lowest_id_wins_when_two_mcs_are_eligible() {
        let mut s = McSelector::new();
        let start = t0();
        for id in ["MC-2", "MC-1"] {
            s.on_role(id, true);
            s.on_heartbeat(id, start);
            s.on_heartbeat(id, start + Duration::from_secs(3));
        }
        assert_eq!(
            s.evaluate(start + Duration::from_secs(3)).as_deref(),
            Some("MC-1")
        );
    }
}
