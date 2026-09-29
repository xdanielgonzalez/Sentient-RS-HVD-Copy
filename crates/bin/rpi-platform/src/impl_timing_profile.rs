use super::prelude::*;

impl TimingProfile {
    /// The real MIL-STD-3071 spec timings — what every shipping (release) build uses, always, and
    /// what a debug build uses unless `SENTIENT_TEST_FAST_TIMERS` is set. See this type's doc
    /// comment for the full test-only-override contract.
    pub const fn production() -> Self {
        TimingProfile {
            heartbeat_deadline: Duration::from_secs(3),
            new_mc: Duration::from_secs(3),
            lost_active_mc: Duration::from_secs(6),
            no_mc: Duration::from_secs(10),
            rate_limit_period: Duration::from_secs(10),
            eval_tick: Duration::from_millis(500),
        }
    }

    /// Compressed timings for e2e tests ONLY — never reachable from a release build (see
    /// [`TimingProfile::resolve`]). Proportionally similar to [`TimingProfile::production`] (same
    /// New-MC : Lost-Active-MC : No-MC ratios) but ~10x-20x faster, so a binary-spawning e2e test
    /// observes the same state-machine transitions in a few hundred milliseconds to a couple of
    /// seconds instead of tens of seconds.
    pub const fn fast() -> Self {
        TimingProfile {
            heartbeat_deadline: Duration::from_millis(300),
            new_mc: Duration::from_millis(300),
            lost_active_mc: Duration::from_millis(600),
            no_mc: Duration::from_millis(1000),
            rate_limit_period: Duration::from_millis(1000),
            eval_tick: Duration::from_millis(50),
        }
    }

    /// Resolve the timing profile the process should run with — called ONCE, at startup
    /// (`bin_rpi::runtime::supervise`), and threaded down into every consumer (`HostPlatform`,
    /// `McSelector`, the `mc_select` task) from there. NEVER re-read per-request or per-cycle.
    ///
    /// Returns [`TimingProfile::production`] unconditionally in a release build — the
    /// `#[cfg(debug_assertions)]` gate below means the `SENTIENT_TEST_FAST_TIMERS` environment
    /// read does not exist in the compiled release binary at all, so no runtime environment can
    /// ever compress the shipping adapter's spec timings. In a debug build (`cargo test`,
    /// `cargo build` without `--release`), setting `SENTIENT_TEST_FAST_TIMERS` (to any value)
    /// switches to [`TimingProfile::fast`] for fast, deterministic e2e test runs; leaving it unset
    /// keeps debug builds spec-conformant too (e.g. `cargo test -p bin-rpi` unit tests).
    pub fn resolve() -> Self {
        #[cfg(debug_assertions)]
        {
            if std::env::var_os("SENTIENT_TEST_FAST_TIMERS").is_some() {
                return Self::fast();
            }
        }
        Self::production()
    }
}

#[cfg(test)]
mod tests {
    use super::TimingProfile;
    use std::time::Duration;

    #[test]
    fn production_matches_the_documented_spec_values() {
        let p = TimingProfile::production();
        assert_eq!(p.heartbeat_deadline, Duration::from_secs(3));
        assert_eq!(p.new_mc, Duration::from_secs(3));
        assert_eq!(p.lost_active_mc, Duration::from_secs(6));
        assert_eq!(p.no_mc, Duration::from_secs(10));
        assert_eq!(p.rate_limit_period, Duration::from_secs(10));
        assert_eq!(p.eval_tick, Duration::from_millis(500));
    }

    #[test]
    fn fast_is_strictly_faster_than_production_in_every_field() {
        let prod = TimingProfile::production();
        let fast = TimingProfile::fast();
        assert!(fast.heartbeat_deadline < prod.heartbeat_deadline);
        assert!(fast.new_mc < prod.new_mc);
        assert!(fast.lost_active_mc < prod.lost_active_mc);
        assert!(fast.no_mc < prod.no_mc);
        assert!(fast.rate_limit_period < prod.rate_limit_period);
        assert!(fast.eval_tick < prod.eval_tick);
    }

    /// Both halves of the env-gate live in ONE test (rather than two) so the process-global
    /// `SENTIENT_TEST_FAST_TIMERS` var is only ever mutated serially within a single test
    /// function — no race against a sibling test over the same var.
    #[test]
    fn resolve_respects_the_debug_only_fast_env_gate() {
        std::env::remove_var("SENTIENT_TEST_FAST_TIMERS");
        assert_eq!(TimingProfile::resolve(), TimingProfile::production());

        std::env::set_var("SENTIENT_TEST_FAST_TIMERS", "1");
        #[cfg(debug_assertions)]
        assert_eq!(TimingProfile::resolve(), TimingProfile::fast());
        #[cfg(not(debug_assertions))]
        assert_eq!(TimingProfile::resolve(), TimingProfile::production());
        std::env::remove_var("SENTIENT_TEST_FAST_TIMERS");
    }
}
