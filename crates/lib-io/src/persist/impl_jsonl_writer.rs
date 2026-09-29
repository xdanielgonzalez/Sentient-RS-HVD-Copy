use std::io;
use std::path::PathBuf;

use super::prelude::*;

impl JsonlWriter {
    /// Build a fresh writer for `policy`. No file is opened yet — that happens lazily on the first
    /// [`JsonlWriter::append`] (see `ensure_open`).
    pub fn new(policy: JsonlPolicy) -> Self {
        Self {
            policy,
            state: DiskState::default(),
            backend: None,
        }
    }

    /// Path of the always-current active log file (`{dir}/{prefix}.jsonl`; rotated segments are
    /// `{dir}/{prefix}.{index}.jsonl` — see `impl_rotation.rs`).
    pub(crate) fn active_path(&self) -> PathBuf {
        self.policy
            .dir
            .join(format!("{}.jsonl", self.policy.file_prefix))
    }

    /// Make sure `policy.dir` exists and a backend is open on the active file. A failure here is
    /// handled identically to a write-time failure by every caller (see `write_line`) — never
    /// panics, never propagates past this crate's boundary.
    pub(crate) fn ensure_open(&mut self) -> io::Result<()> {
        if self.backend.is_some() {
            return Ok(());
        }
        std::fs::create_dir_all(&self.policy.dir)?;
        let path = self.active_path();
        let backend = FsBackend::open(&path)?;
        self.state.bytes_written = backend.len();
        self.state.current_file = Some(path);
        self.backend = Some(Box::new(backend));
        Ok(())
    }

    /// Append one [`LogRecord`] to the active file, handling rotation, disk-full backoff, and
    /// periodic fsync per `policy`. Infallible from the caller's perspective (see the `crate`
    /// module docs) — every failure degrades (`DiskState::disk_full`/`DiskState::dropped`) rather
    /// than propagating or panicking.
    pub fn append(&mut self, record: &LogRecord) {
        let line = format_line(record);
        self.write_line(&line);
    }

    /// Flush + fsync the active file (the drain-on-shutdown path, #55). No-op if nothing is open.
    pub fn flush(&mut self) {
        if let Some(backend) = self.backend.as_mut() {
            if let Err(e) = backend.sync() {
                log::warn!("lib-persist: shutdown fsync failed: {e}");
            }
        }
    }

    pub(crate) fn raw_write(&mut self, line: &[u8]) -> io::Result<()> {
        self.ensure_open()?;
        // `ensure_open` guarantees `Some` on `Ok(())`.
        if let Some(backend) = self.backend.as_mut() {
            backend.write_all(line)?;
        }
        Ok(())
    }

    /// `pub(crate)`, not private: shared with `impl_capture_writer.rs`'s `JsonlWriter::append_capture`
    /// so the raw-capture path reuses this same rotation/disk-full/fsync machinery on an
    /// already-formatted line, rather than duplicating it.
    pub(crate) fn write_line(&mut self, line: &[u8]) {
        if self.state.disk_full && !self.probe_due() {
            self.state.dropped += 1;
            return;
        }
        if self.attempt_write(line) {
            self.state.bytes_written = self.backend.as_ref().map_or(0, |b| b.len());
            self.state.lines_written += 1;
            self.maybe_fsync();
            self.maybe_rotate();
        } else {
            self.state.dropped += 1;
        }
    }

    fn maybe_fsync(&mut self) {
        if self.policy.fsync_every_lines == 0 {
            return;
        }
        if !self
            .state
            .lines_written
            .is_multiple_of(self.policy.fsync_every_lines)
        {
            return;
        }
        if let Some(backend) = self.backend.as_mut() {
            if let Err(e) = backend.sync() {
                log::warn!("lib-persist: periodic fsync failed: {e}");
            }
        }
    }
}

/// Render `record` as one JSONL line (envelope shape:
/// `{"ts":..,"bus":"..","topic":"..","payload":<verbatim JSON>}`), trailing newline included.
/// `payload` is embedded byte-for-byte via [`RawValue`] — never re-escaped — since it is already
/// rendered TMS JSON as published.
fn format_line(record: &LogRecord) -> Vec<u8> {
    match std::str::from_utf8(&record.payload)
        .ok()
        .and_then(|s| RawValue::from_string(s.to_owned()).ok())
    {
        Some(raw) => {
            let env = Envelope {
                ts: record.ts_ms,
                bus: &record.bus,
                topic: &record.topic,
                payload: &raw,
            };
            match serde_json::to_vec(&env) {
                Ok(mut line) => {
                    line.push(b'\n');
                    line
                }
                Err(_) => fallback_line(record),
            }
        }
        None => fallback_line(record),
    }
}

/// Defensive fallback for a payload that isn't valid UTF-8/JSON — never expected in production
/// (`render_tms`/`render_fault` always emit valid JSON — see `impl-app`'s `host::prelude`), but
/// this must still never panic on garbage input. Re-encodes the payload as an escaped JSON string
/// (lossy UTF-8) instead of embedding it raw, so the envelope line itself always stays valid JSON.
fn fallback_line(record: &LogRecord) -> Vec<u8> {
    let lossy = String::from_utf8_lossy(&record.payload);
    let payload_json = serde_json::to_string(lossy.as_ref()).unwrap_or_default();
    let bus_json = serde_json::to_string(&record.bus).unwrap_or_default();
    let topic_json = serde_json::to_string(&record.topic).unwrap_or_default();
    format!(
        "{{\"ts\":{},\"bus\":{bus_json},\"topic\":{topic_json},\"payload\":{payload_json}}}\n",
        record.ts_ms
    )
    .into_bytes()
}

#[cfg(test)]
// Test code may unwrap/panic; the workspace lint gate denies these in production logic only.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// A `WriteBackend` fake for the ENOSPC-recovery path (#56) — real disk-full conditions aren't
    /// practical to trigger in a unit test, so this is the seam `impl_disk_full.rs`'s doc comment
    /// points at. `fail_writes` counts down; while nonzero, `write_all` reports a disk-full error.
    struct FakeBackend {
        fail_writes: usize,
        len: u64,
    }

    impl WriteBackend for FakeBackend {
        fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
            if self.fail_writes > 0 {
                self.fail_writes -= 1;
                return Err(io::Error::from(io::ErrorKind::StorageFull));
            }
            self.len += buf.len() as u64;
            Ok(())
        }
        fn sync(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn len(&self) -> u64 {
            self.len
        }
    }

    fn fresh_tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("lib-persist-tests-{}", std::process::id()))
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn test_policy(dir: &std::path::Path) -> JsonlPolicy {
        JsonlPolicy {
            dir: dir.to_path_buf(),
            file_prefix: "tms".to_string(),
            max_file_bytes: 200,
            max_files: 2,
            fsync_every_lines: 1,
            queue_depth: 64,
            full_probe_ms: 50,
        }
    }

    fn sample_record(i: u64) -> LogRecord {
        LogRecord {
            ts_ms: 1_000 + i,
            bus: "pack-a".to_string(),
            topic: format!("tms/reading/{i}"),
            payload: serde_json::to_vec(&serde_json::json!({"idx": i, "v": 3.5})).expect("json"),
        }
    }

    // ---- envelope shape ---------------------------------------------------------------------

    #[test]
    fn envelope_embeds_payload_verbatim_not_re_escaped() {
        let record = sample_record(7);
        let line = format_line(&record);
        assert_eq!(*line.last().expect("non-empty line"), b'\n');
        let value: serde_json::Value =
            serde_json::from_slice(&line[..line.len() - 1]).expect("valid JSON line");
        assert_eq!(value["ts"], 1_007);
        assert_eq!(value["bus"], "pack-a");
        assert_eq!(value["topic"], "tms/reading/7");
        // The payload must be the SAME JSON structure, not a JSON-encoded string of it.
        let expected_payload: serde_json::Value =
            serde_json::from_slice(&record.payload).expect("fixture payload is valid JSON");
        assert_eq!(value["payload"], expected_payload);
        assert!(
            value["payload"].is_object(),
            "payload must be embedded raw, not as a string"
        );
    }

    #[test]
    fn malformed_payload_falls_back_without_panicking() {
        let record = LogRecord {
            ts_ms: 1,
            bus: "pack-a".to_string(),
            topic: "tms/x".to_string(),
            payload: vec![
                0xFF, 0xFE, b'{', b'n', b'o', b't', b' ', b'j', b's', b'o', b'n',
            ],
        };
        let line = format_line(&record);
        let value: serde_json::Value =
            serde_json::from_slice(&line[..line.len() - 1]).expect("fallback still valid JSON");
        assert_eq!(value["ts"], 1);
        assert!(
            value["payload"].is_string(),
            "malformed payload degrades to a JSON string"
        );
    }

    // ---- rotation + bounded footprint + deletion order --------------------------------------

    #[test]
    fn rotation_bounds_total_files_and_deletes_oldest_first() {
        let dir = fresh_tmp_dir("rotation");
        let policy = test_policy(&dir);
        let mut writer = JsonlWriter::new(policy);
        for i in 0..80u64 {
            writer.append(&sample_record(i));
        }
        assert_eq!(
            writer.state.dropped, 0,
            "no drops expected writing to a healthy real tempdir"
        );
        assert!(
            writer.state.files_rotated > 2,
            "80 records past a 200-byte max_file_bytes must force multiple rotations, got {}",
            writer.state.files_rotated
        );
        let rotated = writer.list_rotated().expect("list rotated segments");
        assert!(
            rotated.len() <= 2,
            "must never retain more than max_files=2 rotated segments, got {}",
            rotated.len()
        );
        if let Some((oldest_idx, _)) = rotated.first() {
            assert!(
                *oldest_idx > 1,
                "the very first rotated segment (index 1) should have been pruned; oldest \
                 surviving is index {oldest_idx}"
            );
        }
        // Bounded footprint: max_file_bytes * (max_files + 1), with slack for exactly one line
        // since a file only rotates once a write pushes it AT/OVER the threshold (the line that
        // tips it over is already included). Computed from the actual fixture, not a guessed
        // constant, so this stays correct if the envelope shape or fixture ever changes size.
        let max_line_len = (0..80u64)
            .map(|i| format_line(&sample_record(i)).len() as u64)
            .max()
            .expect("at least one fixture line");
        let mut total: u64 = std::fs::metadata(writer.active_path()).map_or(0, |m| m.len());
        for (_, path) in &rotated {
            total += std::fs::metadata(path).map_or(0, |m| m.len());
        }
        let budget = 200 * 3 + max_line_len * 3; // one tipping-line's slack per surviving file
        assert!(
            total <= budget,
            "on-disk footprint {total} exceeds the bounded budget {budget}"
        );
    }

    // ---- drain / flush ------------------------------------------------------------------------

    #[test]
    fn flush_makes_prior_writes_readable_back() {
        let dir = fresh_tmp_dir("flush");
        // `test_policy`'s `max_file_bytes: 200` is sized for the rotation-boundary tests, not this
        // one: 3 fixture lines run ~237 bytes, which would trip `maybe_rotate` on the 3rd write and
        // roll the very lines this test asserts on into `tms.1.jsonl`, leaving the active file it
        // reads back empty. Widen the budget so flush/read-back is exercised in isolation from
        // rotation (rotation itself is covered by `rotation_bounds_total_files_and_deletes_oldest_first`).
        let policy = JsonlPolicy {
            max_file_bytes: 10_000,
            ..test_policy(&dir)
        };
        let mut writer = JsonlWriter::new(policy);
        for i in 0..3u64 {
            writer.append(&sample_record(i));
        }
        writer.flush();
        let contents = std::fs::read_to_string(writer.active_path()).expect("read active file");
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 3);
        for (i, line) in lines.iter().enumerate() {
            let value: serde_json::Value = serde_json::from_str(line).expect("valid JSON line");
            assert_eq!(value["topic"], format!("tms/reading/{i}"));
        }
    }

    // ---- disk-full / ENOSPC recovery (#56), via the WriteBackend fake -----------------------

    #[test]
    fn enospc_with_nothing_to_prune_enters_degraded_and_drops() {
        let dir = fresh_tmp_dir("enospc-nothing-to-prune");
        let policy = test_policy(&dir);
        let mut writer = JsonlWriter::new(policy);
        writer.backend = Some(Box::new(FakeBackend {
            fail_writes: usize::MAX,
            len: 0,
        }));
        writer.append(&sample_record(0));
        assert!(writer.state.disk_full);
        assert_eq!(writer.state.dropped, 1);
    }

    #[test]
    fn enospc_recovers_after_pruning_the_oldest_rotated_segment() {
        let dir = fresh_tmp_dir("enospc-recover");
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("tms.1.jsonl"), b"old\n").expect("seed a rotated segment");
        let policy = test_policy(&dir);
        let mut writer = JsonlWriter::new(policy);
        writer.backend = Some(Box::new(FakeBackend {
            fail_writes: 1, // first write fails, retry after pruning succeeds
            len: 0,
        }));
        writer.append(&sample_record(0));
        assert!(
            !writer.state.disk_full,
            "must recover once the retry succeeds"
        );
        assert_eq!(writer.state.dropped, 0);
        assert!(
            !dir.join("tms.1.jsonl").exists(),
            "the oldest rotated segment must have been pruned to free space"
        );
    }

    #[test]
    fn while_probe_window_open_records_drop_without_attempting_a_write() {
        let dir = fresh_tmp_dir("probe-gate");
        let mut policy = test_policy(&dir);
        policy.full_probe_ms = 60_000;
        let mut writer = JsonlWriter::new(policy);
        writer.state.disk_full = true;
        writer.state.next_probe_at =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(60));
        writer.append(&sample_record(0));
        assert_eq!(writer.state.dropped, 1);
        assert!(
            writer.backend.is_none(),
            "no write should have been attempted while the probe window is still open"
        );
    }
}
