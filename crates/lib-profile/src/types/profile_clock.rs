//! Abstract clock interface for the profiler.

/// Abstract clock interface: get current time in microseconds.
pub trait ProfileClock {
    /// Return current time in microseconds.
    fn now_micros(&self) -> u64;
}
