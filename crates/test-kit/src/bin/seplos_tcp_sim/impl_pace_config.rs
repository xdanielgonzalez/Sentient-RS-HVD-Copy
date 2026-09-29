use super::prelude::*;

impl PaceConfig {
    /// Default device-processing/RS-485 turnaround delay applied when `--pace` is given without
    /// an explicit `--turnaround-ms` (real Seplos turnaround is roughly 5-20ms).
    pub(crate) const DEFAULT_TURNAROUND_MS: u64 = 10;
}
