use std::path::PathBuf;
use std::time::Instant;

/// Rotation + disk-full bookkeeping for the active jsonl file.
#[derive(Debug, Clone, Default)]
pub struct DiskState {
    /// Path of the currently-open log file, if any has been opened yet.
    pub current_file: Option<PathBuf>,
    /// Bytes written to the current file since it was opened.
    pub bytes_written: u64,
    /// Lines written to the current file since it was opened (drives `fsync_every_lines`).
    pub lines_written: usize,
    /// Total files rotated (created) over this writer's lifetime.
    pub files_rotated: usize,
    /// True while a disk-full/write-error condition is being backed off from (see
    /// `JsonlPolicy::full_probe_ms`).
    pub disk_full: bool,
    /// Records dropped after being dequeued from the channel (disk-full / write failure) —
    /// distinct from `JsonlHandle::dropped`, which counts records the *sender* dropped because the
    /// channel itself was already full (never reached the writer at all).
    pub dropped: u64,
    /// Wall-clock instant this writer may next attempt a real write while `disk_full` — bounds how
    /// often a persistently-full/broken disk is retried to `JsonlPolicy::full_probe_ms`. `None`
    /// when not currently degraded (every write is attempted for real).
    pub next_probe_at: Option<Instant>,
}
