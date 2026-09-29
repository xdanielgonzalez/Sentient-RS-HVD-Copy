use crate::prelude::*;

impl EmbassyClock {
    /// Capture the boot instant. Call at most once, before the poll loop spawns (the same
    /// single-init-before-multithreading discipline the retired Zephyr path's `MonitorLogger::new`
    /// required) — `main` upholds this (constructed exactly once at startup).
    pub fn new() -> Self {
        EmbassyClock {
            boot: Instant::now(),
        }
    }
}

impl McuClock for EmbassyClock {
    fn now(&self) -> ClockMonotonic {
        let ms = self.boot.elapsed().as_millis();
        ClockMonotonic {
            seconds: (ms / 1000) as u32,
            nanoseconds: ((ms % 1000) * 1_000_000) as u32,
        }
    }

    async fn sleep_ms(&mut self, ms: u32) {
        // Liveness bump: `impl_app::run`'s poll loop calls `platform.sleep(..)` exactly once per
        // iteration, success or backoff (see `crate::POLL_HEARTBEAT`'s doc comment) — this is a
        // pure "the loop is still iterating" signal `watchdog_task` gates its feed on, not a
        // telemetry-success signal (an absent BMS must not stop the bump, or the device would
        // reset-loop instead of merely logging degraded telemetry).
        crate::POLL_HEARTBEAT.fetch_add(1, Ordering::Relaxed);
        Timer::after_millis(ms as u64).await;
    }
}
