use super::prelude::*;
use std::sync::atomic::Ordering;

impl CyclePulse {
    /// Record that this task completed one cycle — bumps the generation counter
    /// [`WatchdogAgg::run`] watches for staleness.
    pub fn pet(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    /// The current generation counter (monotonically increasing; wraps are fine — `run` only
    /// compares for *change*, never an absolute value).
    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}
