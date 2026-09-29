//! Type definitions for the profiler.

pub use self::profile_clock::ProfileClock;
pub use self::profile_sink::ProfileSink;
pub(crate) use self::span_entry::SpanEntry;
pub use self::span_registry::SpanRegistry;
pub use self::span_stats::SpanStats;

#[cfg(feature = "profile")]
pub use self::measure_guard::MeasureGuard;

#[cfg(feature = "profile")]
mod measure_guard;
mod profile_clock;
mod profile_sink;
mod span_entry;
mod span_registry;
mod span_stats;
