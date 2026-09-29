use std::fmt::Write as _;

use super::prelude::*;

impl JsonlWriter {
    /// Append one [`RawCaptureRecord`] to the active file as one hex-encoded JSONL line, reusing
    /// the SAME rotation/disk-full/fsync machinery [`JsonlWriter::append`] drives for
    /// [`LogRecord`] (see `write_line` in `impl_jsonl_writer.rs`) — only the line-formatting
    /// differs. A formatting failure (never expected in practice — see `format_capture_line`) is
    /// counted the same way a disk-full drop is (`DiskState::dropped`), never panics.
    pub(crate) fn append_capture(&mut self, record: &RawCaptureRecord) {
        match format_capture_line(record) {
            Some(line) => self.write_line(&line),
            None => self.state.dropped += 1,
        }
    }
}

/// Render `record` as one JSONL line:
/// `{"ts":..,"bus":"..","request":"<hex>","response":"<hex>","valid":..}`. Hex (not base64) is
/// deliberate: `(`/`)`-framed ASCII Voltronic/Seplos protocol bytes stay directly grep-able in the
/// raw capture file (e.g. `grep '2851504947'` for a literal `(QPIG` prefix), which a base64
/// encoding would obscure.
///
/// Returns `None` only if serialization itself fails — practically unreachable (every field is
/// either a plain integer/bool or a string built from this crate's own hex/UTF-8-safe `bus`
/// field), but handled without panicking rather than assumed away (see the workspace's
/// `unwrap_used`/`expect_used` deny lints).
fn format_capture_line(record: &RawCaptureRecord) -> Option<Vec<u8>> {
    let request_hex = to_hex(&record.request);
    let response_hex = to_hex(&record.response);
    let env = CaptureEnvelope {
        ts: record.ts_ms,
        bus: &record.bus,
        request: &request_hex,
        response: &response_hex,
        valid: record.valid,
    };
    match serde_json::to_vec(&env) {
        Ok(mut line) => {
            line.push(b'\n');
            Some(line)
        }
        Err(e) => {
            log::warn!("lib-persist: capture record serialization failed (dropping): {e}");
            None
        }
    }
}

/// Lowercase hex-encode `bytes` (e.g. `[0x2b, 0x0d]` -> `"2b0d"`), no separators. No `hex` crate
/// dependency for a two-line encode — see `Cargo.toml`'s minimal dependency list.
fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        // `write!` into a `String` via `fmt::Write` never fails.
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
// Test code may unwrap/panic; the workspace lint gate denies these in production logic only.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Decode a lowercase hex string back to bytes — the inverse of `to_hex`, kept test-only since
    /// nothing in production needs to decode a capture file back (a future replay/analysis tool
    /// would own its own decoder).
    fn from_hex(s: &str) -> Option<Vec<u8>> {
        if !s.len().is_multiple_of(2) {
            return None;
        }
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
            .collect()
    }

    fn fresh_tmp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join(format!(
                "lib-persist-capture-writer-tests-{}",
                std::process::id()
            ))
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn test_policy(dir: &std::path::Path) -> JsonlPolicy {
        JsonlPolicy {
            dir: dir.to_path_buf(),
            file_prefix: "capture".to_string(),
            max_file_bytes: 200,
            max_files: 2,
            fsync_every_lines: 1,
            queue_depth: 64,
            full_probe_ms: 50,
        }
    }

    fn sample_record(i: u64, valid: bool) -> RawCaptureRecord {
        RawCaptureRecord {
            ts_ms: 1_000 + i,
            bus: "pack-a".to_string(),
            request: vec![0x28, b'Q', b'P', b'I', b'G', b'S', 0x0d],
            response: format!("(idx-{i}\r").into_bytes(),
            valid,
        }
    }

    // ---- hex encode/decode round trip --------------------------------------------------------

    #[test]
    fn hex_round_trips_request_and_response_bytes() {
        let record = sample_record(0, true);
        let hex_req = to_hex(&record.request);
        let hex_resp = to_hex(&record.response);
        assert_eq!(from_hex(&hex_req).expect("valid hex"), record.request);
        assert_eq!(from_hex(&hex_resp).expect("valid hex"), record.response);
    }

    #[test]
    fn hex_round_trips_empty_and_arbitrary_byte_values() {
        // Every byte value 0..=255, including 0x00 and 0x0d (the frame terminator) and non-ASCII
        // bytes — hex must never assume ASCII-only content.
        let all_bytes: Vec<u8> = (0..=255u8).collect();
        let hex = to_hex(&all_bytes);
        assert_eq!(hex.len(), all_bytes.len() * 2);
        assert_eq!(from_hex(&hex).expect("valid hex"), all_bytes);
        assert_eq!(to_hex(&[]), "");
        assert_eq!(from_hex("").expect("empty is valid"), Vec::<u8>::new());
    }

    // ---- envelope shape -----------------------------------------------------------------------

    #[test]
    fn envelope_hex_encodes_request_and_response_and_keeps_ts_bus_valid() {
        let record = sample_record(7, false);
        let line = format_capture_line(&record).expect("serialization succeeds");
        assert_eq!(*line.last().expect("non-empty line"), b'\n');
        let value: serde_json::Value =
            serde_json::from_slice(&line[..line.len() - 1]).expect("valid JSON line");
        assert_eq!(value["ts"], 1_007);
        assert_eq!(value["bus"], "pack-a");
        assert_eq!(value["valid"], false);
        let req_hex = value["request"].as_str().expect("request is a hex string");
        let resp_hex = value["response"]
            .as_str()
            .expect("response is a hex string");
        assert_eq!(from_hex(req_hex).expect("valid hex"), record.request);
        assert_eq!(from_hex(resp_hex).expect("valid hex"), record.response);
        // Grep-ability: the literal ASCII command bytes must appear as their hex digits, not
        // base64 (this is the whole point of choosing hex — see `format_capture_line`'s doc
        // comment).
        assert_eq!(req_hex, "2851504947530d");
    }

    // ---- rotation reuse (shared machinery with the LogRecord/`run_writer` path) --------------

    #[test]
    fn rotation_and_disk_full_machinery_is_reused_for_capture_records() {
        let dir = fresh_tmp_dir("rotation-reuse");
        let policy = test_policy(&dir);
        let mut writer = JsonlWriter::new(policy);
        for i in 0..80u64 {
            writer.append_capture(&sample_record(i, i.is_multiple_of(5)));
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
    }

    #[test]
    fn flush_makes_capture_records_readable_back_in_order() {
        let dir = fresh_tmp_dir("flush");
        let policy = JsonlPolicy {
            max_file_bytes: 10_000,
            ..test_policy(&dir)
        };
        let mut writer = JsonlWriter::new(policy);
        for i in 0..3u64 {
            writer.append_capture(&sample_record(i, true));
        }
        writer.flush();
        let contents = std::fs::read_to_string(writer.active_path()).expect("read active file");
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 3);
        for (i, line) in lines.iter().enumerate() {
            let value: serde_json::Value = serde_json::from_str(line).expect("valid JSON line");
            assert_eq!(value["ts"], 1_000 + i as u64);
        }
    }
}
