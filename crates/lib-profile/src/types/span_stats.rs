//! A single span's aggregated statistics snapshot.

/// A single span's aggregated statistics.
#[derive(Clone, Copy, Debug)]
pub struct SpanStats {
    /// Name of the span.
    pub name: &'static str,
    /// Number of times measured.
    pub count: u32,
    /// Minimum duration in microseconds.
    pub min_micros: u64,
    /// Maximum duration in microseconds.
    pub max_micros: u64,
    /// Total duration in microseconds.
    pub sum_micros: u64,
}
