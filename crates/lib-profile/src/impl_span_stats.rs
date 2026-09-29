use crate::types::SpanStats;

impl SpanStats {
    /// Compute mean duration in microseconds (sum / count), or 0 if count is 0.
    pub fn mean_micros(&self) -> u64 {
        if self.count > 0 {
            self.sum_micros / (self.count as u64)
        } else {
            0
        }
    }
}
