use super::CaptureConfig;
use lib_io::persist::JsonlPolicy;

/// Not configurable — fixed so `capture.jsonl`/`capture.1.jsonl`/... never collides with the
/// `[disk]` tail's `tms.jsonl` on the same filesystem.
const CAPTURE_FILE_PREFIX: &str = "capture";
/// Not configurable via `[capture]` — mirrors `DiskConfig`'s default
/// (`default_disk_fsync_every_lines`); raw captures don't need their own separate knob for this.
const CAPTURE_FSYNC_EVERY_LINES: usize = 100;
/// Not configurable via `[capture]` — mirrors `DiskConfig`'s default (`default_disk_full_probe_ms`).
const CAPTURE_FULL_PROBE_MS: u64 = 5_000;

impl CaptureConfig {
    /// Map to `lib_io::persist`'s rotation/backpressure policy `lib_io::persist::run_capture_writer`
    /// takes. Called from `runtime.rs`'s `supervise` when `[capture]` is present (see
    /// `CaptureConfig`'s doc comment).
    ///
    /// `max_total_bytes` (a byte budget) is converted to `JsonlPolicy::max_files` (a rotated-file
    /// COUNT): `JsonlWriter::prune_rotated`'s doc comment gives the footprint bound as
    /// `max_file_bytes * (max_files + 1)` — the `+1` is the active file's own share of the budget
    /// — so `max_files = max_total_bytes / max_file_bytes`, saturating-subtracting that 1. A
    /// budget tighter than one file's worth floors to `max_files = 0` (only the active file is
    /// ever retained; each rotation immediately prunes the just-rotated segment) rather than
    /// underflowing.
    pub fn to_policy(&self) -> JsonlPolicy {
        let files_in_budget = if self.max_file_bytes == 0 {
            0
        } else {
            self.max_total_bytes / self.max_file_bytes
        };
        JsonlPolicy {
            dir: self.dir.clone().into(),
            file_prefix: CAPTURE_FILE_PREFIX.to_string(),
            max_file_bytes: self.max_file_bytes,
            max_files: usize::try_from(files_in_budget.saturating_sub(1)).unwrap_or(usize::MAX),
            fsync_every_lines: CAPTURE_FSYNC_EVERY_LINES,
            queue_depth: self.queue_depth,
            full_probe_ms: CAPTURE_FULL_PROBE_MS,
        }
    }

    /// Reject `max_file_bytes = 0` — same degenerate-rotation-churn hazard as
    /// `DiskConfig::validate` (`JsonlWriter::maybe_rotate` rolls to a new file on every single
    /// write when the size bound is `0`). `max_total_bytes` is deliberately NOT bounded here: `0`
    /// (or any budget tighter than one file) already floors gracefully to `max_files = 0` (see
    /// `to_policy` above and its `to_policy_floors_max_files_at_zero_...` test) — a legitimate
    /// "retain no rotated history" choice for a bounded diagnostics capture, not a hazard.
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.max_file_bytes == 0 {
            anyhow::bail!(
                "config error: [capture].max_file_bytes = 0 rotates to a new file on every single \
                 write (constant rename + prune churn) — set a real per-file size budget"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
// Test code may unwrap/panic; the workspace lint gate denies these in production logic only.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn cfg(max_file_bytes: u64, max_total_bytes: u64) -> CaptureConfig {
        CaptureConfig {
            dir: "/var/lib/sentient/capture".to_string(),
            max_file_bytes,
            max_total_bytes,
            queue_depth: 64,
        }
    }

    #[test]
    fn to_policy_maps_fields_and_fixes_the_file_prefix() {
        let policy = cfg(10 * 1024 * 1024, 100 * 1024 * 1024).to_policy();
        assert_eq!(
            policy.dir,
            std::path::PathBuf::from("/var/lib/sentient/capture")
        );
        assert_eq!(policy.file_prefix, "capture");
        assert_eq!(policy.max_file_bytes, 10 * 1024 * 1024);
        assert_eq!(policy.queue_depth, 64);
        assert_eq!(policy.fsync_every_lines, CAPTURE_FSYNC_EVERY_LINES);
        assert_eq!(policy.full_probe_ms, CAPTURE_FULL_PROBE_MS);
    }

    #[test]
    fn to_policy_converts_max_total_bytes_budget_to_a_rotated_file_count() {
        // 100MB budget / 10MB per file = 10 files total (active + rotated); max_files is the
        // ROTATED count, so 10 - 1 = 9.
        let policy = cfg(10 * 1024 * 1024, 100 * 1024 * 1024).to_policy();
        assert_eq!(policy.max_files, 9);
    }

    #[test]
    fn to_policy_floors_max_files_at_zero_when_budget_is_tighter_than_one_file() {
        // A 1MB total budget against a 10MB max_file_bytes must never underflow to a huge
        // usize — it floors to 0 rotated segments retained.
        let policy = cfg(10 * 1024 * 1024, 1024 * 1024).to_policy();
        assert_eq!(policy.max_files, 0);
    }

    #[test]
    fn to_policy_treats_zero_max_file_bytes_as_zero_budget_without_dividing_by_zero() {
        let policy = cfg(0, 100 * 1024 * 1024).to_policy();
        assert_eq!(policy.max_files, 0);
    }

    #[test]
    fn validate_rejects_zero_max_file_bytes() {
        assert!(cfg(0, 100 * 1024 * 1024).validate().is_err());
    }

    #[test]
    fn validate_accepts_the_smallest_nonzero_max_file_bytes() {
        assert!(cfg(1, 100 * 1024 * 1024).validate().is_ok());
    }

    #[test]
    fn validate_accepts_a_max_total_bytes_tighter_than_one_file() {
        // `max_total_bytes` is deliberately unbounded — a budget tighter than one file already
        // floors gracefully (see `to_policy_floors_max_files_at_zero_when_budget_is_tighter_...`).
        assert!(cfg(10 * 1024 * 1024, 0).validate().is_ok());
    }
}
