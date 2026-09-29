//! RAII guard for recording a span measurement on drop.

use super::profile_clock::ProfileClock;
use super::span_registry::SpanRegistry;

/// Guard that records a span measurement on drop.
///
/// Records elapsed time when dropped (RAII pattern).
#[cfg(feature = "profile")]
pub struct MeasureGuard<'a, const N: usize> {
    pub(crate) registry: &'a SpanRegistry<N>,
    pub(crate) name: &'static str,
    pub(crate) clock: &'a dyn ProfileClock,
    pub(crate) start_micros: u64,
}
