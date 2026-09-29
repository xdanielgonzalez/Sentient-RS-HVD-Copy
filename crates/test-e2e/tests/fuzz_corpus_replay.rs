//! `fuzz_corpus_replay` — turns the `test-e2e/fuzz` cargo-fuzz targets' coverage into a
//! permanent, CI-gating regression test that runs on **stable** (no nightly, no libFuzzer).
//!
//! Per #99's fuzz-relocation comment: the libFuzzer *targets* must stay in the cargo-fuzz crate
//! (a nightly/tooling constraint cargo test can't satisfy), but the *regression coverage* — "never
//! panic on this seed/crash corpus" — belongs here, in the normal test lane, so it runs on every
//! PR rather than only on the weekly scheduled fuzz job.
//!
//! This replays the committed seed corpus (`tests/fixtures/fuzz_corpus/`, checked into git —
//! distinct from cargo-fuzz's own gitignored runtime `fuzz/corpus/`/`fuzz/artifacts/`) through the
//! exact same real call chains the three libFuzzer targets exercise
//! (`fuzz_parse_telemetry`, `fuzz_parse_command`, `fuzz_canonical` — see `test-e2e/fuzz/
//! fuzz_targets/`): `lib_parse::seplos::frame::ResponseFrame::parse`, `telemetry::
//! decode_reading`, `fault::decode_fault`, `command::build_command`, and the canonical -> TMS ->
//! JSON mapping. None of these may panic on any input, malformed or not — that is the contract
//! under test; a `Result::Err` is a perfectly fine outcome, a panic is not.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use heapless::String as HString;
use lib_mapping::{
    codec::to_json_slice,
    schema::ClockMonotonic,
    transform::{ToDiagnostics, ToTms},
};
use lib_parse::seplos::{
    command::{build_command, Command},
    fault,
    frame::ResponseFrame,
    telemetry,
};

/// The committed fuzz seed/crash corpus, checked into `tests/fixtures/fuzz_corpus/`.
const CORPUS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fuzz_corpus");

/// Strip at most one trailing `\n` / `\r\n` (an artifact of the seed files being plain-text
/// checked-in fixtures) — never load-bearing for the "never panics" contract under test, just
/// keeps the "well-formed" seeds decodable as intended.
fn trimmed(raw: &[u8]) -> &[u8] {
    let raw = raw.strip_suffix(b"\n").unwrap_or(raw);
    raw.strip_suffix(b"\r").unwrap_or(raw)
}

/// Mirrors `fuzz_targets/fuzz_parse_telemetry.rs`: parse, then (if it parsed) decode telemetry.
/// Never panics.
fn replay_parse_telemetry(data: &[u8]) {
    if let Ok(frame) = ResponseFrame::parse(data) {
        let _ = telemetry::decode_reading(&frame);
    }
}

/// Mirrors `fuzz_targets/fuzz_parse_command.rs`: direct decode fuzzing on the raw bytes, plus an
/// encode/decode round trip keyed off the first two bytes. Never panics.
fn replay_parse_command(data: &[u8]) {
    let _ = ResponseFrame::parse(data);

    if data.is_empty() {
        return;
    }
    let dip = data[0];
    let command = match data.get(1).copied().unwrap_or(0) % 3 {
        0 => Command::Telemetry,
        1 => Command::Fault,
        _ => Command::SerialNumber,
    };

    let mut out: heapless::Vec<u8, 32> = heapless::Vec::new();
    if build_command(dip, command, &mut out).is_ok() {
        let _ = ResponseFrame::parse(&out);
    }
}

/// Mirrors `fuzz_targets/fuzz_canonical.rs`: the full canonical -> TMS -> JSON mapping chain, off
/// the real parse chain (never a hand-built `CanonicalReading`/`CanonicalFault`). Never panics and
/// never overruns the bounded JSON output buffer.
fn replay_canonical(data: &[u8]) {
    let Ok(frame) = ResponseFrame::parse(data) else {
        return;
    };

    let now = ClockMonotonic {
        seconds: 0,
        nanoseconds: 0,
    };
    let mut device_id: HString<32> = HString::new();
    let _ = device_id.push_str("fuzz-device");

    if let Ok(reading) = telemetry::decode_reading(&frame) {
        let dc = reading.to_dc_measurement(device_id.clone(), now);
        let hw = reading.to_power_hardware(device_id.clone(), now);

        let mut buf = [0u8; 4096];
        let _ = to_json_slice(&dc, &mut buf);
        let _ = to_json_slice(&hw, &mut buf);
    }

    if let Ok(f) = fault::decode_fault(&frame) {
        let diag = f.to_active_diagnostics(device_id, now);
        let mut buf = [0u8; 4096];
        let _ = to_json_slice(&diag, &mut buf);
    }
}

/// Every seed file's bytes through all three replayed call chains — asserts nothing panics.
#[test]
fn corpus_replay_never_panics() {
    let mut seeds = 0usize;
    for entry in std::fs::read_dir(CORPUS_DIR).expect("read the committed fuzz seed corpus dir") {
        let entry = entry.expect("read corpus dir entry");
        if !entry.file_type().expect("file type").is_file() {
            continue;
        }
        let raw = std::fs::read(entry.path()).expect("read seed file");
        let data = trimmed(&raw);

        replay_parse_telemetry(data);
        replay_parse_command(data);
        replay_canonical(data);
        seeds += 1;
    }
    assert!(
        seeds > 0,
        "the committed fuzz seed corpus at {CORPUS_DIR} must not be empty"
    );
}

/// The empty input is its own edge case worth pinning explicitly (not just via a zero-length
/// corpus file, which is awkward to check into git): every replayed chain must handle it
/// gracefully.
#[test]
fn empty_input_never_panics() {
    let data: &[u8] = &[];
    replay_parse_telemetry(data);
    replay_parse_command(data);
    replay_canonical(data);
}
