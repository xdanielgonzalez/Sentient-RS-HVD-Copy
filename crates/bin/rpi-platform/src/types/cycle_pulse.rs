use std::sync::atomic::AtomicU64;
use std::sync::Arc;

/// A liveness pulse one background task increments each time it completes a cycle;
/// [`super::watchdog_agg::WatchdogAgg`] aggregates one `CyclePulse` per named task so a single
/// host-side watchdog check can see whether every configured secondary task is still alive, not
/// just the primary poll loop.
#[derive(Clone, Default)]
pub struct CyclePulse(pub Arc<AtomicU64>);
