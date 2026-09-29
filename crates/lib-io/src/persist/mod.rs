//! `persist` — disk / JSON-lines telemetry logging (Scope A).
//!
//! Folded from the former standalone `lib-persist` crate (#285) into `lib-io` as a `host`-gated
//! module — `persist` is std-only (tokio/serde_json) and must never enter the `no_std` MCU build
//! of this crate, so it is compiled only under `#[cfg(feature = "host")]` (see `crate::lib.rs`).
//!
//! A second `lib_mapping::sink::Publisher`-adjacent target: every published TMS payload is
//! tailed into a rotating `*.jsonl` log on local storage, so a field engineer (or the contract's
//! microSD demo) can inspect telemetry history without a network hop. The write path is bounded
//! (a fixed-depth channel + [`JsonlPolicy::queue_depth`]) and never blocks the poll loop — a full
//! queue drops the oldest/newest record (see [`JsonlHandle::dropped`]) and logs a warning rather
//! than back-pressuring telemetry.
//!
//! [`WriteBackend`] is the seam over the actual filesystem write path, so ENOSPC/EIO disk-full
//! behavior is host-testable against a fake backend without a real full disk (see
//! `impl_disk_full.rs`).
//!
//! A second, independent writer ([`run_capture_writer`]) tails raw PRE-parse wire exchanges
//! ([`RawCaptureRecord`]) rather than rendered TMS JSON — a field-diagnostics tool for decision
//! 0001's open risk (no real Voltronic/Seplos wire captures were available when the parsers were
//! derived from protocol docs). It reuses the same [`JsonlWriter`] rotation/disk-full/fsync
//! machinery, only the line format differs (hex-encoded request/response instead of the
//! [`LogRecord`] envelope).
//!
//! std-only (the RPi deployment target — see CLAUDE.md "Platform & Deployment"); never reachable
//! from a `no_std` build of this crate.
#![forbid(unsafe_code)]

mod prelude;
mod types;

mod impl_capture_writer;
mod impl_disk_full;
mod impl_fs_backend;
mod impl_jsonl_handle;
mod impl_jsonl_writer;
mod impl_rotation;

pub use types::*;

/// Drive the jsonl writer: drain `rx` and persist each [`LogRecord`] through `policy`'s
/// rotation/disk-full rules (see [`JsonlWriter::append`]) until `shutdown` reports true or every
/// sender is dropped, then drain whatever is left queued and fsync before returning (the
/// drain-on-shutdown path, #55) — this never leaves telemetry that was already handed to
/// `JsonlHandle` unwritten just because a shutdown raced the last few records.
pub async fn run_writer(
    mut rx: tokio::sync::mpsc::Receiver<LogRecord>,
    policy: JsonlPolicy,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    log::info!(
        "lib-persist: run_writer started (dir={}, prefix={})",
        policy.dir.display(),
        policy.file_prefix
    );
    let mut writer = JsonlWriter::new(policy);
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                // An `Err` here means the sender was dropped without ever signaling shutdown
                // (shouldn't happen in production — `supervise` always sends `true` before
                // returning — but treating it as a shutdown too avoids a busy-loop on a future
                // that would otherwise resolve immediately forever).
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
            maybe = rx.recv() => {
                match maybe {
                    Some(record) => writer.append(&record),
                    None => break,
                }
            }
        }
    }
    // Drain-on-shutdown (#55): flush whatever is already queued — bounded by the channel's own
    // depth, never unbounded work — before the final fsync, so a shutdown racing the last few
    // published records doesn't lose them.
    while let Ok(record) = rx.try_recv() {
        writer.append(&record);
    }
    writer.flush();
    log::info!(
        "lib-persist: run_writer stopped (lines_written={}, dropped={}, files_rotated={})",
        writer.state.lines_written,
        writer.state.dropped,
        writer.state.files_rotated
    );
}

/// Drive the raw-capture writer: drain `rx` and persist each [`RawCaptureRecord`] through
/// `policy`'s rotation/disk-full rules (see `JsonlWriter::append_capture`) until `shutdown`
/// reports true or every sender is dropped, then drain whatever is left queued and fsync before
/// returning — mirrors [`run_writer`]'s drain-on-shutdown contract (#55) exactly, just against
/// [`RawCaptureRecord`] instead of [`LogRecord`].
pub async fn run_capture_writer(
    mut rx: tokio::sync::mpsc::Receiver<RawCaptureRecord>,
    policy: JsonlPolicy,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    log::info!(
        "lib-persist: run_capture_writer started (dir={}, prefix={})",
        policy.dir.display(),
        policy.file_prefix
    );
    let mut writer = JsonlWriter::new(policy);
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                // Same rationale as `run_writer`: a dropped sender that never signaled shutdown
                // is treated as a shutdown too, to avoid busy-looping on an already-resolved
                // future forever.
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
            maybe = rx.recv() => {
                match maybe {
                    Some(record) => writer.append_capture(&record),
                    None => break,
                }
            }
        }
    }
    // Drain-on-shutdown (#55, same contract as `run_writer`).
    while let Ok(record) = rx.try_recv() {
        writer.append_capture(&record);
    }
    writer.flush();
    log::info!(
        "lib-persist: run_capture_writer stopped (lines_written={}, dropped={}, files_rotated={})",
        writer.state.lines_written,
        writer.state.dropped,
        writer.state.files_rotated
    );
}

#[cfg(test)]
// Test code may unwrap/panic; the workspace lint gate denies these in production logic only.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn fresh_tmp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join(format!(
                "lib-persist-run-writer-tests-{}",
                std::process::id()
            ))
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn policy(dir: &std::path::Path) -> JsonlPolicy {
        JsonlPolicy {
            dir: dir.to_path_buf(),
            file_prefix: "tms".to_string(),
            max_file_bytes: 1_000_000,
            max_files: 3,
            fsync_every_lines: 1,
            queue_depth: 32,
            full_probe_ms: 50,
        }
    }

    fn record(i: u64) -> LogRecord {
        LogRecord {
            ts_ms: i,
            bus: "pack-a".to_string(),
            topic: format!("tms/reading/{i}"),
            payload: serde_json::to_vec(&serde_json::json!({"idx": i})).expect("json"),
        }
    }

    /// Every record handed to the channel before shutdown must land on disk — the drain-on-
    /// shutdown path (#55) — whether shutdown races the last few sends or not.
    #[tokio::test]
    async fn drains_queued_records_and_fsyncs_on_shutdown() {
        let dir = fresh_tmp_dir("drain-on-shutdown");
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let handle = tokio::spawn(run_writer(rx, policy(&dir), shutdown_rx));

        for i in 0..5u64 {
            tx.send(record(i)).await.expect("channel accepts record");
        }
        // Signal shutdown without waiting for the writer to have drained anything yet — the
        // still-queued sends must not be lost.
        shutdown_tx.send(true).expect("send shutdown");
        drop(tx);
        handle.await.expect("run_writer task must not panic");

        let contents = std::fs::read_to_string(dir.join("tms.jsonl")).expect("read active file");
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(
            lines.len(),
            5,
            "every queued record must have been written before exit"
        );
        for (i, line) in lines.iter().enumerate() {
            let value: serde_json::Value = serde_json::from_str(line).expect("valid JSON line");
            assert_eq!(value["topic"], format!("tms/reading/{i}"));
        }
    }

    /// Dropping every sender (without ever signaling shutdown) must also let the writer drain and
    /// exit cleanly, matching a scenario where the whole process is torn down mid-run.
    #[tokio::test]
    async fn exits_cleanly_once_every_sender_is_dropped() {
        let dir = fresh_tmp_dir("sender-dropped");
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let handle = tokio::spawn(run_writer(rx, policy(&dir), shutdown_rx));

        tx.send(record(0)).await.expect("channel accepts record");
        drop(tx);
        handle.await.expect("run_writer task must not panic");

        let contents = std::fs::read_to_string(dir.join("tms.jsonl")).expect("read active file");
        assert_eq!(contents.lines().count(), 1);
    }

    // ---- run_capture_writer (end-to-end; mirrors the run_writer tests above) -----------------
    //
    // `run_capture_writer` isn't spawned anywhere in this workstream (the `SerialTransport`
    // decorator that would feed it is a separate follow-on) — these tests are the thing that
    // actually exercises it end-to-end, rather than leaving it unused-but-pub.

    fn capture_policy(dir: &std::path::Path) -> JsonlPolicy {
        JsonlPolicy {
            dir: dir.to_path_buf(),
            file_prefix: "capture".to_string(),
            max_file_bytes: 1_000_000,
            max_files: 3,
            fsync_every_lines: 1,
            queue_depth: 32,
            full_probe_ms: 50,
        }
    }

    fn capture_record(i: u64) -> RawCaptureRecord {
        RawCaptureRecord {
            ts_ms: i,
            bus: "pack-a".to_string(),
            request: vec![0x28, b'Q', b'I', b'D', 0x0d],
            response: format!("(idx-{i}\r").into_bytes(),
            valid: true,
        }
    }

    /// Every capture record handed to the channel before shutdown must land on disk — the same
    /// drain-on-shutdown path (#55) `run_writer` guarantees, proven here for `run_capture_writer`.
    #[tokio::test]
    async fn capture_writer_drains_queued_records_and_fsyncs_on_shutdown() {
        let dir = fresh_tmp_dir("capture-drain-on-shutdown");
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let handle = tokio::spawn(run_capture_writer(rx, capture_policy(&dir), shutdown_rx));

        for i in 0..5u64 {
            tx.send(capture_record(i))
                .await
                .expect("channel accepts record");
        }
        shutdown_tx.send(true).expect("send shutdown");
        drop(tx);
        handle
            .await
            .expect("run_capture_writer task must not panic");

        let contents =
            std::fs::read_to_string(dir.join("capture.jsonl")).expect("read active file");
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(
            lines.len(),
            5,
            "every queued capture record must have been written before exit"
        );
        for (i, line) in lines.iter().enumerate() {
            let value: serde_json::Value = serde_json::from_str(line).expect("valid JSON line");
            assert_eq!(value["ts"], i as u64);
            assert_eq!(value["bus"], "pack-a");
            assert_eq!(value["valid"], true);
            // hex, not base64 — request is always the fixed `(QID\r` command (0x28 'Q' 'I' 'D'
            // 0x0d).
            assert_eq!(value["request"], "285149440d");
        }
    }

    /// Dropping every sender must also let `run_capture_writer` drain and exit cleanly.
    #[tokio::test]
    async fn capture_writer_exits_cleanly_once_every_sender_is_dropped() {
        let dir = fresh_tmp_dir("capture-sender-dropped");
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let handle = tokio::spawn(run_capture_writer(rx, capture_policy(&dir), shutdown_rx));

        tx.send(capture_record(0))
            .await
            .expect("channel accepts record");
        drop(tx);
        handle
            .await
            .expect("run_capture_writer task must not panic");

        let contents =
            std::fs::read_to_string(dir.join("capture.jsonl")).expect("read active file");
        assert_eq!(contents.lines().count(), 1);
    }
}
