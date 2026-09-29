//! Abstract output sink for profiler reports.

/// Abstract output sink for profiler reports.
pub trait ProfileSink {
    /// Emit a single line of output (e.g., to defmt, RTT, or a string buffer).
    fn log_line(&self, line: &str);
}
