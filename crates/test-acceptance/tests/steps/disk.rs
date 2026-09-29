//! Steps for the disk / JSON-lines telemetry logging feature area (Scope A, #55/#56/#57) — see
//! `docs/gap-analysis-meteor-parity.md`'s "Disk / JSON-lines telemetry logging" finding.
//!
//! GREP-GATE — NO re-implemented rotation/disk-full/write logic lives in this file. Every scenario
//! spawns the REAL `lib_io::persist::run_writer` task (the exact production entry point
//! `bin-rpi::runtime::supervise` wires up) against a real temp directory, feeds it real
//! `lib_io::persist::LogRecord`s over the real bounded channel, and only then inspects the resulting
//! `*.jsonl` files. What lives here is Gherkin<->Rust plumbing only: deterministic fixture record
//! generation, and read-only filesystem parsing (listing/reading the files the writer produced) for
//! assertions — never a second rotation/backoff algorithm.
//!
//! No `SpecWorld` scenario state is used: each scenario's steps agree on a literal, named scratch
//! directory (from the step text itself), and the real files that `run_writer` produces there ARE
//! the state shared between steps — so a `When` step can run the writer to completion and a later
//! `Then` step can independently re-derive the same expected fixture and compare it against what
//! actually landed on disk.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use cucumber::{given, then, when};
use lib_io::persist::{JsonlPolicy, LogRecord};

use crate::SpecWorld;

/// Scratch directory for one named disk-logging scenario, isolated per test-binary process run
/// (`std::process::id()`) so concurrent/repeat runs never collide.
fn scratch_dir(name: &str) -> PathBuf {
    std::env::temp_dir()
        .join(format!(
            "sentient-disk-logging-acceptance-{}",
            std::process::id()
        ))
        .join(name)
}

/// Deterministic telemetry-record fixture: pure and reproducible so a `When` step (publishing) and
/// a later `Then` step (asserting) can each independently regenerate the exact same expected
/// records without any state bridged through `SpecWorld`.
fn fixture_records(n: usize) -> Vec<LogRecord> {
    (0..n)
        .map(|i| LogRecord {
            ts_ms: 1_700_000_000_000 + i as u64,
            bus: "pack-a".to_string(),
            topic: format!("tms/METEOR-001/reading/{i}"),
            payload: serde_json::to_vec(&serde_json::json!({ "idx": i, "value": i * 10 }))
                .expect("fixture payload is valid JSON"),
        })
        .collect()
}

/// List `{prefix}.{index}.jsonl` rotated segments in `dir`, as `(index, path)` pairs — a read-only
/// filesystem observation used only for assertions, not a reimplementation of `lib_io::persist`'s
/// own rotation bookkeeping (that decision-making is exercised for real by the `run_writer` task under
/// test in the `When` step).
fn list_rotated(dir: &Path, prefix: &str) -> Vec<(usize, PathBuf)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    let want_prefix = format!("{prefix}.");
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(rest) = name.strip_prefix(&want_prefix) else {
            continue;
        };
        let Some(idx_str) = rest.strip_suffix(".jsonl") else {
            continue;
        };
        if let Ok(idx) = idx_str.parse::<usize>() {
            out.push((idx, entry.path()));
        }
    }
    out
}

// ---- Given: scratch-dir fixtures -----------------------------------------------------------

#[given(expr = "a fresh disk log scratch dir {string}")]
fn given_fresh_scratch_dir(_world: &mut SpecWorld, name: String) {
    let _ = std::fs::remove_dir_all(scratch_dir(&name));
}

#[given(expr = "a disk log directory {string} that cannot be written to")]
fn given_unwritable_scratch_dir(_world: &mut SpecWorld, name: String) {
    let dir = scratch_dir(&name);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&dir);
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent).expect("create scratch parent dir");
    }
    // Occupy the log directory's own path with a plain FILE, so `ensure_open`'s
    // `create_dir_all(dir)` fails with a structural "wrong type at this path" error. Deliberately
    // NOT a chmod'd-read-only directory: this test suite (like CI) may run as root, which bypasses
    // ordinary directory-permission checks entirely — a permission-bit approach would silently
    // pass without exercising anything. A non-directory occupying the path blocks writes
    // regardless of the running user's privilege level.
    std::fs::write(&dir, b"occupying this path so it cannot become a directory")
        .expect("create the blocking file");
}

// ---- When: run the real writer end-to-end ----------------------------------------------------

#[when(
    expr = "{int} telemetry records are published to a disk log in scratch dir {string} with \
             max_file_bytes {int} and max_files {int}"
)]
async fn when_records_published(
    _world: &mut SpecWorld,
    n: i64,
    name: String,
    max_file_bytes: i64,
    max_files: i64,
) {
    let policy = JsonlPolicy {
        dir: scratch_dir(&name),
        file_prefix: "tms".to_string(),
        max_file_bytes: max_file_bytes as u64,
        max_files: max_files as usize,
        fsync_every_lines: 1,
        queue_depth: 128,
        full_probe_ms: 50,
    };
    let (tx, rx) = tokio::sync::mpsc::channel(128);
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let handle = tokio::spawn(lib_io::persist::run_writer(rx, policy, shutdown_rx));

    for record in fixture_records(n as usize) {
        tx.send(record).await.expect("channel accepts record");
    }
    drop(tx);
    let _ = shutdown_tx.send(true);
    // Reaching this `.expect` without panicking IS part of the "never crashes" acceptance
    // criterion — a panicked writer task surfaces here as a `JoinError`.
    handle
        .await
        .expect("disk-logging writer task must not panic");
}

// ---- Then: inspect the real files the writer produced -----------------------------------------

#[then(expr = "scratch dir {string} contains {int} JSONL lines")]
fn then_contains_n_lines(_world: &mut SpecWorld, name: String, n: i64) {
    let path = scratch_dir(&name).join("tms.jsonl");
    let contents =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert_eq!(contents.lines().count(), n as usize);
}

#[then(
    expr = "every JSONL line in scratch dir {string} carries its published ts, bus, topic, and \
             payload verbatim"
)]
fn then_lines_match_fixture(_world: &mut SpecWorld, name: String) {
    let path = scratch_dir(&name).join("tms.jsonl");
    let contents = std::fs::read_to_string(&path).expect("read active file");
    let lines: Vec<&str> = contents.lines().collect();
    let expected = fixture_records(lines.len());
    for (line, record) in lines.iter().zip(expected.iter()) {
        let value: serde_json::Value = serde_json::from_str(line).expect("valid JSON line");
        assert_eq!(value["ts"], record.ts_ms, "ts mismatch");
        assert_eq!(value["bus"], record.bus, "bus mismatch");
        assert_eq!(value["topic"], record.topic, "topic mismatch");
        let expected_payload: serde_json::Value =
            serde_json::from_slice(&record.payload).expect("fixture payload is valid JSON");
        assert_eq!(
            value["payload"], expected_payload,
            "payload must be embedded verbatim, not re-escaped"
        );
    }
}

#[then(expr = "scratch dir {string} retains at most {int} rotated segments plus the active file")]
fn then_bounded_rotated_segments(_world: &mut SpecWorld, name: String, max_files: i64) {
    let dir = scratch_dir(&name);
    let rotated = list_rotated(&dir, "tms");
    assert!(
        (rotated.len() as i64) <= max_files,
        "expected at most {max_files} rotated segments, found {}",
        rotated.len()
    );
    assert!(
        dir.join("tms.jsonl").exists(),
        "the active file must still exist"
    );
}

#[then(
    expr = "the oldest surviving segment in scratch dir {string} is not the very first one written"
)]
fn then_oldest_segment_was_pruned(_world: &mut SpecWorld, name: String) {
    let dir = scratch_dir(&name);
    let rotated = list_rotated(&dir, "tms");
    let oldest = rotated.into_iter().map(|(idx, _)| idx).min();
    match oldest {
        Some(idx) => assert!(
            idx > 1,
            "the very first rotated segment (index 1) should have been pruned by now"
        ),
        None => panic!("expected at least one surviving rotated segment"),
    }
}

#[then("the disk-logging pipeline completes without panicking or hanging")]
fn then_pipeline_completed(_world: &mut SpecWorld) {
    // Reaching this step at all is the proof: the preceding `When` step already awaited the
    // writer task's `JoinHandle` to completion and would itself have panicked (failing the
    // scenario) had the task panicked, deadlocked, or blocked the publishing side.
}

#[then(expr = "no JSONL file was created in scratch dir {string}")]
fn then_no_jsonl_file(_world: &mut SpecWorld, name: String) {
    let path = scratch_dir(&name).join("tms.jsonl");
    assert!(
        !path.exists(),
        "no file should have been created in an unwritable directory"
    );
}
