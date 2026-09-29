use std::io;
use std::time::{Duration, Instant};

use super::prelude::*;

/// ENOSPC on Linux (the only shipping target — CLAUDE.md "Platform & Deployment") — the fallback
/// match for hosts/std versions where `io::Error::kind()` doesn't yet classify it as
/// [`io::ErrorKind::StorageFull`].
const ENOSPC: i32 = 28;

/// True if `e` represents a disk-full condition, matched both via the modern
/// `std::io::ErrorKind::StorageFull` and the raw `ENOSPC` errno.
fn is_disk_full(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::StorageFull || e.raw_os_error() == Some(ENOSPC)
}

impl JsonlWriter {
    /// True while a previous disk-full/error backoff window (`JsonlPolicy::full_probe_ms`) has NOT
    /// elapsed yet — callers should drop-and-count without attempting a real write.
    pub(crate) fn probe_due(&self) -> bool {
        match self.state.next_probe_at {
            Some(at) => Instant::now() >= at,
            None => true,
        }
    }

    /// Enter (or extend) the disk-full/degraded state: logs once per transition into degraded
    /// (throttled — never once per dropped record), schedules the next probe `full_probe_ms` out.
    fn enter_degraded(&mut self, reason: &str) {
        let was_already_degraded = self.state.disk_full;
        self.state.disk_full = true;
        self.state.next_probe_at =
            Some(Instant::now() + Duration::from_millis(self.policy.full_probe_ms));
        if !was_already_degraded {
            log::warn!(
                "lib-persist: degrading ({reason}); records will be dropped and re-probed every \
                 {}ms until a write succeeds again",
                self.policy.full_probe_ms
            );
        }
    }

    /// Clear the degraded state (a write succeeded again) and log the recovery once.
    fn clear_degraded(&mut self) {
        if self.state.disk_full {
            log::info!("lib-persist: write condition recovered; resuming normal writes");
        }
        self.state.disk_full = false;
        self.state.next_probe_at = None;
    }

    /// Try to write `line`, recovering once from an `ENOSPC` failure per the contract (#56): delete
    /// the oldest rotated segment and retry the write exactly once. Any other IO error (or a
    /// failed retry) enters the degraded state. Returns whether `line` ended up written.
    pub(crate) fn attempt_write(&mut self, line: &[u8]) -> bool {
        match self.raw_write(line) {
            Ok(()) => {
                self.clear_degraded();
                true
            }
            Err(e) if is_disk_full(&e) => self.recover_from_disk_full(line),
            Err(e) => {
                log::warn!("lib-persist: write failed: {e}");
                // Don't keep hammering a backend that just errored (e.g. EIO) — the next write
                // attempt (once the probe window allows it) reopens fresh via `ensure_open`.
                self.backend = None;
                self.enter_degraded(&format!("write error: {e}"));
                false
            }
        }
    }

    fn recover_from_disk_full(&mut self, line: &[u8]) -> bool {
        match self.delete_oldest_rotated() {
            Ok(true) => match self.raw_write(line) {
                Ok(()) => {
                    self.clear_degraded();
                    true
                }
                Err(e) => {
                    self.enter_degraded(&format!(
                        "disk full; retry after pruning oldest segment also failed: {e}"
                    ));
                    false
                }
            },
            Ok(false) => {
                self.enter_degraded("disk full and no rotated segment left to prune");
                false
            }
            Err(e) => {
                self.enter_degraded(&format!("disk full; pruning oldest segment failed: {e}"));
                false
            }
        }
    }
}
