use super::prelude::*;

impl WatchdogAgg {
    /// Register a new named background task, returning the [`CyclePulse`] it should increment on
    /// every completed cycle.
    pub fn register(&mut self, name: impl Into<String>) -> CyclePulse {
        let pulse = CyclePulse::default();
        self.slots.push((name.into(), pulse.clone()));
        pulse
    }

    /// Drive the aggregated watchdog feed forever: every second, check every registered slot's
    /// [`CyclePulse`] for a change since it was last observed. Feeds `sd_notify`'s `WATCHDOG=1`
    /// ONLY while every slot has pulsed within `window_ms` of its last change — a single stalled
    /// background task (disk writer, mesh radio, ...) withholds the feed, so systemd's
    /// `WatchdogSec=` restarts the whole process rather than limping along with one dead task.
    ///
    /// Bug #172's fix (see `HostPlatform::on_cycle`'s doc comment) still holds: liveness here is
    /// "did the slot's counter change", never `Health::healthy()` — a telemetry-degraded (but
    /// still cycling) primary poll loop keeps petting its `CyclePulse` and keeps feeding the
    /// watchdog, exactly as before this aggregation existed. `self.slots` empty means no
    /// registered background tasks — the feed is fed unconditionally every tick, matching the
    /// pre-aggregation single-task behavior.
    pub async fn run(self, window_ms: u64) -> ! {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        let mut last_value: Vec<u64> = self.slots.iter().map(|(_, p)| p.get()).collect();
        let now0 = Instant::now();
        let mut last_changed: Vec<Instant> = vec![now0; self.slots.len()];
        loop {
            interval.tick().await;
            let now = Instant::now();
            let mut all_fresh = true;
            for (i, (name, pulse)) in self.slots.iter().enumerate() {
                let v = pulse.get();
                if v != last_value[i] {
                    last_value[i] = v;
                    last_changed[i] = now;
                }
                let age_ms = now.duration_since(last_changed[i]).as_millis() as u64;
                if age_ms > window_ms {
                    all_fresh = false;
                    log::warn!("watchdog: task '{name}' stale ({age_ms} ms since last pulse)");
                }
            }
            if all_fresh {
                let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Watchdog]);
            }
        }
    }
}
