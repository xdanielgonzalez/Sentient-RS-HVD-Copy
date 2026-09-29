use super::DiskConfig;
use lib_io::persist::JsonlPolicy;

impl DiskConfig {
    /// Map to `lib_io::persist`'s rotation/backpressure policy `lib_io::persist::run_writer` takes.
    pub fn to_policy(&self) -> JsonlPolicy {
        JsonlPolicy {
            dir: self.dir.clone().into(),
            file_prefix: self.file_prefix.clone(),
            max_file_bytes: self.max_file_bytes,
            max_files: self.max_files,
            fsync_every_lines: self.fsync_every_lines,
            queue_depth: self.queue_depth,
            full_probe_ms: self.full_probe_ms,
        }
    }

    /// Reject `max_file_bytes = 0`.
    ///
    /// `JsonlWriter::maybe_rotate` rolls to a new file whenever `bytes_written < max_file_bytes` is
    /// false — with `max_file_bytes = 0` that is *never* true, so every single line written
    /// triggers a rotation (a rename + a `prune_rotated` disk scan) forever: constant file
    /// churn, not a "roll immediately" feature anyone would intend. Fails loudly rather than
    /// clamping — matching this config layer's established "fail loudly, don't silently correct"
    /// posture (see `PollConfig::validate`/`DdsToml::validate`).
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.max_file_bytes == 0 {
            anyhow::bail!(
                "config error: [disk].max_file_bytes = 0 rotates to a new file on every single \
                 line written (constant rename + prune churn) — set a real per-file size budget"
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

    fn cfg(max_file_bytes: u64) -> DiskConfig {
        DiskConfig {
            dir: "/var/lib/sentient/telemetry".to_string(),
            file_prefix: "tms".to_string(),
            max_file_bytes,
            max_files: 10,
            fsync_every_lines: 100,
            queue_depth: 256,
            full_probe_ms: 5000,
        }
    }

    #[test]
    fn validate_rejects_zero_max_file_bytes() {
        assert!(cfg(0).validate().is_err());
    }

    #[test]
    fn validate_accepts_the_smallest_nonzero_value() {
        assert!(cfg(1).validate().is_ok());
    }
}
