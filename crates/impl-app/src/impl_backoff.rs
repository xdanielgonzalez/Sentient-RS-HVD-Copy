use crate::Backoff;

impl Backoff {
    /// Backoff floor: the first dead-cycle sleep.
    pub const MIN_MS: u32 = 1_000;
    /// Default backoff ceiling (the Pi's "truly-absent BMS" cap).
    pub const MAX_MS: u32 = 30_000;

    /// A fresh backoff, parked at the floor, ceilinged at [`Self::MAX_MS`].
    pub const fn new() -> Self {
        Self::with_max(Self::MAX_MS)
    }
    /// A fresh backoff, parked at the floor, ceilinged at `max_ms` instead of the default
    /// [`Self::MAX_MS`] — lets a platform (e.g. the MCU demo) tighten the retry ceiling so
    /// telemetry recovers fast from a transient fault, without changing the ramp shape.
    pub const fn with_max(max_ms: u32) -> Self {
        Backoff {
            cur_ms: Self::MIN_MS,
            max_ms,
        }
    }
    /// The current delay, then double toward the cap for next time.
    pub fn next_delay(&mut self) -> u32 {
        let now = self.cur_ms;
        self.cur_ms = self.cur_ms.saturating_mul(2).min(self.max_ms);
        now
    }
    /// Reset to the floor (called on the first successful exchange after a dead streak).
    pub fn reset(&mut self) {
        self.cur_ms = Self::MIN_MS;
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Backoff::new()
    }
}
