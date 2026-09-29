use std::path::PathBuf;

/// Rotation + backpressure policy for the jsonl telemetry log.
#[derive(Debug, Clone)]
pub struct JsonlPolicy {
    /// Directory the rotating log files live in.
    pub dir: PathBuf,
    /// Filename prefix (e.g. `tms` -> `tms.jsonl` active, rotated to `tms.1.jsonl`, `tms.2.jsonl`, ..).
    pub file_prefix: String,
    /// Roll to a new file once the active file reaches this many bytes.
    pub max_file_bytes: u64,
    /// Maximum number of rotated files retained (oldest deleted past this).
    pub max_files: usize,
    /// `fsync` the active file every N lines written (0 disables periodic fsync).
    pub fsync_every_lines: usize,
    /// Bounded channel depth between the poll loop and the writer task.
    pub queue_depth: usize,
    /// How often (ms) to re-probe a disk-full condition before resuming writes.
    pub full_probe_ms: u64,
}
