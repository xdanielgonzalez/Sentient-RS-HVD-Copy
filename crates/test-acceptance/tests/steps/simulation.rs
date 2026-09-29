//! Steps for the simulation feature area (#226 stage W2): Gherkin scenarios define the simulated
//! Seplos device — its battery state, its fault-injection behavior, its protocol responses — at
//! RUNTIME, so positive/negative/edge simulation is expressible as human-readable spec instead of
//! Rust constants.
//!
//! GREP-GATE — NO battery-state construction logic lives in this file. Every `BatteryState` is
//! built through `test_kit::seplos::parse_battery_state_spec` (the W1 shared spec-grammar
//! parser/builder) — never a hand-assembled struct literal, and never a new compile-time
//! constant. Every decode is a direct call into the real, reused crates: `test_kit::seplos`
//! (`SeplosSim`/`Behavior`/`SimReply`, the shared-checksum simulator), `lib_parse::seplos`
//! (`Seplos`, `Command`, `build_command`, the real decoder), and `lib_parse::vendor::ParseError`
//! (the real vendor-neutral typed error). What lives here is Gherkin<->Rust plumbing only:
//! turning a Gherkin table into the spec-grammar's `key=value` text, assembling malformed
//! *request*-frame fixtures for the auto-NAK scenarios (frame math delegated to the real
//! `lib_parse::seplos::checksum` impl, mirroring `resilience.rs::make_response` /
//! `lib-parse/tests/seplos_hardening.rs::make_response` — never re-derived here), and asserting
//! on typed outcomes.
//!
//! In-process only: no sockets, no ptys, no threads beyond the cucumber runner's own — every
//! scenario here drives `SeplosSim::respond` and `lib_parse`'s decoder as plain function calls.
//!
//! Test code only; freely uses unwrap/expect/panic (the workspace lint gate denies these in
//! production logic, not tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cucumber::gherkin::{Step, Table};
use cucumber::{given, then, when};

use heapless::Vec as HVec;
use lib_parse::seplos::checksum::{frame_checksum, length_checksum};
use lib_parse::seplos::command::{build_command, Command};
use lib_parse::seplos::Seplos;
use lib_parse::vendor::{BmsVendorProtocol, ParseError};
use test_kit::seplos::{parse_battery_state_spec, BatteryState, Behavior, SeplosSim, SimReply};

use crate::SpecWorld;

use super::support::error_variant_name;

/// The real sample14 telemetry reply, copied verbatim from
/// `test-kit/tests/battery_state_spec.rs::sample14_encoded_frame_is_byte_identical_after_the_spec_refactor`
/// — pins the accuracy fixture at the acceptance layer too, so a future change to the spec
/// grammar / builder / encoder can't silently drift the wire bytes real downstream fixtures
/// depend on.
const SAMPLE14_TELEMETRY_FRAME: &[u8] = b"~20004600A08E00000E0ED80ED90EDA0EDB0EDC0EDD0EDE0EDF0EE00EE10EE20EE30EE40EE5060BA50BA70BA90BAB0BAD0BAFFF5C14D105730A070803070708000203E8154D0000000000000000DDE9\r";

/// Scenario scratch for the simulation feature area. The battery state / simulator themselves
/// live in the top-level `SpecWorld::state`/`SpecWorld::sim` fields (shared with `mapping.rs`/
/// `resilience.rs`), since this area constructs exactly the same shapes — just at runtime, from a
/// Gherkin table, through the W1 spec grammar.
#[derive(Debug, Default)]
pub struct SimulationState {
    /// Result of building/parsing the most recent battery-state spec: `Some(Ok(()))` on success
    /// (the resulting state already lives in `SpecWorld::state`/`sim`), `Some(Err(message))` for a
    /// spec that fails closed before a simulator can even be built, `None` until a spec `Given`
    /// step has run.
    build_result: Option<Result<(), String>>,
    /// Typed outcome of the most recent `SeplosSim::respond` + decode: `"Ok"`, `"Silence"` (a
    /// genuinely silent bus — no bytes at all), or a `ParseError` variant name.
    outcome: Option<String>,
    /// The RTN/device-error code carried by a `ParseError::DeviceError` outcome, when applicable.
    rtn_code: Option<u8>,
}

// ---- spec-grammar plumbing (Gherkin table/int -> the W1 shared parser) ------------------------

/// Turn a Gherkin data table's rows (after the header row) into the spec grammar's
/// whitespace-separated `key=value` text — the exact input `parse_battery_state_spec` accepts.
fn table_to_spec_text(table: &Table) -> String {
    table
        .rows
        .iter()
        .skip(1) // header row ("key" / "value")
        .map(|row| format!("{}={}", row[0], row[1]))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse `spec_text` through the shared W1 grammar and record the outcome: on success, a fresh
/// `SeplosSim` + the resulting `BatteryState` land in the top-level `SpecWorld` fields (mirroring
/// `mapping.rs`'s `given_14_cell_battery`); on failure, both are cleared so later steps fail
/// loudly instead of reusing a stale simulator.
fn apply_spec(world: &mut SpecWorld, spec_text: &str) {
    match parse_battery_state_spec(spec_text) {
        Ok(state) => {
            world.sim = Some(SeplosSim::new(state.clone()));
            world.state = Some(state);
            world.simulation.build_result = Some(Ok(()));
        }
        Err(e) => {
            world.sim = None;
            world.state = None;
            world.simulation.build_result = Some(Err(e));
        }
    }
}

// ---- request-side fixture builder (auto-NAK scenarios only) -----------------------------------

/// One isolated request-side defect, for the auto-NAK scenarios — mirrors the same
/// single-defect-with-an-otherwise-valid-frame fixture pattern already used by
/// `resilience.rs::make_response` / `lib-parse/tests/seplos_hardening.rs::make_response` (frame
/// math is the real `lib_parse::seplos::checksum` impl, never re-derived here; only ASCII-hex
/// assembly is local).
#[derive(Clone, Copy)]
enum RequestDefect {
    /// The frame CHKSUM no longer matches (LENGTH/LCHKSUM and CID2 untouched).
    BadFrameChecksum,
    /// The LENGTH field's LCHKSUM nibble is wrong (frame CHKSUM computed fresh over the
    /// self-consistent, if logically-wrong, bytes actually sent — isolates this fault alone).
    BadLengthChecksum,
    /// CID2 is a command byte the simulator does not recognize (frame otherwise well-formed).
    BadCid2,
}

fn push_hex_u8(out: &mut Vec<u8>, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push(HEX[(byte >> 4) as usize]);
    out.push(HEX[(byte & 0xF) as usize]);
}

/// Build a DIP-0 telemetry-shaped (`CID2 = 42`) request frame with exactly one `defect` isolated
/// — the repo builder's own 1-hex-char-INFO shape (mirrors `build_command`'s LENID=`001` form).
fn malformed_request(defect: RequestDefect) -> Vec<u8> {
    let lenid: u16 = 1;
    let good_lchksum = length_checksum(lenid);
    let lchksum = match defect {
        RequestDefect::BadLengthChecksum => (good_lchksum ^ 0x1) & 0xF,
        RequestDefect::BadFrameChecksum | RequestDefect::BadCid2 => good_lchksum,
    };
    let length: u16 = ((lchksum as u16) << 12) | (lenid & 0x0FFF);

    let mut body: Vec<u8> = Vec::new();
    body.extend_from_slice(b"20"); // VER
    body.extend_from_slice(b"00"); // ADR (dip 0)
    body.extend_from_slice(b"46"); // CID1
    if matches!(defect, RequestDefect::BadCid2) {
        body.extend_from_slice(b"99"); // not 0x42/0x44/0xA4
    } else {
        body.extend_from_slice(b"42"); // CID2 (telemetry)
    }
    push_hex_u8(&mut body, (length >> 8) as u8);
    push_hex_u8(&mut body, (length & 0xFF) as u8);
    body.push(b'0'); // 1 hex-char INFO (the DIP-0 nibble)

    let mut checksum = frame_checksum(&body);
    if matches!(defect, RequestDefect::BadFrameChecksum) {
        checksum ^= 0x0001;
    }

    let mut frame: Vec<u8> = Vec::new();
    frame.push(b'~');
    frame.extend_from_slice(&body);
    push_hex_u8(&mut frame, (checksum >> 8) as u8);
    push_hex_u8(&mut frame, (checksum & 0xFF) as u8);
    frame.push(b'\r');
    frame
}

// ---- shared respond/decode plumbing ------------------------------------------------------------

/// Send `reply` through the real decoder and record a typed outcome: `"Ok"` (reading decoded into
/// `SpecWorld::reading`), `"Silence"` (no bytes at all — a distinct first-class outcome, not an
/// error frame), or the `ParseError` variant name (with the RTN/device-error code stashed
/// separately when it's a `DeviceError`).
fn record_reply_outcome(world: &mut SpecWorld, dip: u8, reply: SimReply) {
    world.raw_frame = reply.as_bytes().to_vec();
    if world.raw_frame.is_empty() {
        world.simulation.outcome = Some("Silence".to_string());
        world.simulation.rtn_code = None;
        world.reading = None;
        return;
    }
    match Seplos::new().parse_reading(&world.raw_frame, dip) {
        Ok(r) => {
            world.reading = Some(r);
            world.simulation.outcome = Some("Ok".to_string());
            world.simulation.rtn_code = None;
        }
        Err(e) => {
            world.reading = None;
            world.simulation.outcome = Some(error_variant_name(&e));
            world.simulation.rtn_code = match e {
                ParseError::DeviceError(code) => Some(code),
                _ => None,
            };
        }
    }
}

fn poll_telemetry(world: &mut SpecWorld, dip: u8) {
    let mut req: HVec<u8, 32> = HVec::new();
    build_command(dip, Command::Telemetry, &mut req).expect("build telemetry request");
    let sim = world
        .sim
        .as_mut()
        .expect("simulator initialized (a battery-state Given step must run first)");
    let reply = sim.respond(&req);
    record_reply_outcome(world, dip, reply);
}

fn send_malformed_request(world: &mut SpecWorld, defect: RequestDefect) {
    let req = malformed_request(defect);
    let sim = world
        .sim
        .as_mut()
        .expect("simulator initialized (a battery-state Given step must run first)");
    let reply = sim.respond(&req);
    record_reply_outcome(world, 0, reply);
}

// ---- Given ---------------------------------------------------------------------------------

#[given("a battery state defined by the table:")]
fn given_battery_table(world: &mut SpecWorld, step: &Step) {
    let table = step.table.as_ref().expect("step carries a data table");
    let spec_text = table_to_spec_text(table);
    apply_spec(world, &spec_text);
}

#[given(expr = "a battery-state spec requesting {int} cells")]
fn given_spec_cells_count(world: &mut SpecWorld, n: i64) {
    let csv = (0..n)
        .map(|i| (3000 + i).to_string())
        .collect::<Vec<_>>()
        .join(",");
    apply_spec(world, &format!("cells={csv}"));
}

#[given("the default sample14 battery state")]
fn given_sample14(world: &mut SpecWorld) {
    let state = BatteryState::sample_14_cell();
    world.sim = Some(SeplosSim::new(state.clone()));
    world.state = Some(state);
    world.simulation.build_result = Some(Ok(()));
}

#[given("auto-NAK is enabled on the simulator")]
fn given_auto_nak(world: &mut SpecWorld) {
    world
        .sim
        .as_mut()
        .expect("simulator initialized (a battery-state Given step must run first)")
        .set_auto_nak(true);
}

#[given(expr = "the simulator behavior is forced RTN error {int}")]
fn given_forced_rtn(world: &mut SpecWorld, code: i64) {
    world
        .sim
        .as_mut()
        .expect("simulator initialized (a battery-state Given step must run first)")
        .set_behavior(Behavior::RtnError(code as u8));
}

// ---- When ----------------------------------------------------------------------------------

#[when("the adapter polls telemetry")]
fn when_polls_telemetry(world: &mut SpecWorld) {
    poll_telemetry(world, 0);
}

#[when(expr = "the adapter polls telemetry from DIP {int}")]
fn when_polls_telemetry_dip(world: &mut SpecWorld, dip: i64) {
    poll_telemetry(world, dip as u8);
}

#[when("a request with a corrupted frame checksum is sent")]
fn when_bad_frame_checksum(world: &mut SpecWorld) {
    send_malformed_request(world, RequestDefect::BadFrameChecksum);
}

#[when("a request with a corrupted length checksum is sent")]
fn when_bad_length_checksum(world: &mut SpecWorld) {
    send_malformed_request(world, RequestDefect::BadLengthChecksum);
}

#[when("a request with an unrecognized command code is sent")]
fn when_bad_cid2(world: &mut SpecWorld) {
    send_malformed_request(world, RequestDefect::BadCid2);
}

// ---- Then ----------------------------------------------------------------------------------

#[then("the battery-state spec is rejected")]
fn then_spec_rejected(world: &mut SpecWorld) {
    match &world.simulation.build_result {
        Some(Err(_)) => {}
        other => panic!("expected the battery-state spec to be rejected, got {other:?}"),
    }
}

#[then(expr = "the rejection message contains {string}")]
fn then_rejection_contains(world: &mut SpecWorld, needle: String) {
    match &world.simulation.build_result {
        Some(Err(msg)) => assert!(
            msg.contains(&needle),
            "expected rejection message to contain {needle:?}, got {msg:?}"
        ),
        other => panic!("expected a rejected battery-state spec, got {other:?}"),
    }
}

#[then("the canonical reading matches the battery state field-for-field")]
fn then_reading_matches_state(world: &mut SpecWorld) {
    let reading = world
        .reading
        .as_ref()
        .expect("a canonical reading was decoded");
    let state = world.state.as_ref().expect("a battery state was set");
    assert_eq!(
        reading.cell_voltages_mv.as_slice(),
        state.cell_voltages_mv.as_slice(),
        "cell voltages mismatch"
    );
    assert_eq!(
        reading.temperatures_raw.as_slice(),
        state.temperatures_raw.as_slice(),
        "temperatures mismatch"
    );
    assert_eq!(reading.current_ca, state.current_ca, "current mismatch");
    assert_eq!(
        reading.total_voltage_cv, state.total_voltage_cv,
        "total voltage mismatch"
    );
    assert_eq!(
        reading.remaining_capacity_cah, state.remaining_capacity_cah,
        "remaining capacity mismatch"
    );
    assert_eq!(
        reading.typical_capacity_cah, state.typical_capacity_cah,
        "typical capacity mismatch"
    );
    assert_eq!(reading.soc_dpct, state.soc_dpct, "soc mismatch");
    assert_eq!(
        reading.rated_capacity_cah, state.rated_capacity_cah,
        "rated capacity mismatch"
    );
    assert_eq!(reading.cycles, state.cycles, "cycles mismatch");
    assert_eq!(reading.soh_dpct, state.soh_dpct, "soh mismatch");
    assert_eq!(
        reading.port_voltage_cv, state.port_voltage_cv,
        "port voltage mismatch"
    );
}

#[then(expr = "the canonical reading has {int} cells")]
fn then_reading_cell_count(world: &mut SpecWorld, n: i64) {
    let reading = world
        .reading
        .as_ref()
        .expect("a canonical reading was decoded");
    assert_eq!(
        reading.cell_voltages_mv.len(),
        n as usize,
        "cell count mismatch"
    );
}

#[then("the encoded telemetry reply is byte-identical to the accuracy fixture")]
fn then_frame_byte_identical(world: &mut SpecWorld) {
    assert_eq!(
        world.raw_frame.as_slice(),
        SAMPLE14_TELEMETRY_FRAME,
        "sample14 telemetry reply must stay byte-identical to the accuracy fixture"
    );
}

#[then(expr = "the telemetry outcome is {word}")]
fn then_telemetry_outcome(world: &mut SpecWorld, expected: String) {
    assert_eq!(
        world.simulation.outcome.as_deref(),
        Some(expected.as_str()),
        "wrong telemetry outcome"
    );
}

#[then(expr = "the reply is a device error with code {int}")]
fn then_device_error_code(world: &mut SpecWorld, code: i64) {
    assert_eq!(
        world.simulation.outcome.as_deref(),
        Some("DeviceError"),
        "expected a device-error outcome, got {:?}",
        world.simulation.outcome
    );
    assert_eq!(
        world.simulation.rtn_code,
        Some(code as u8),
        "wrong RTN/device-error code"
    );
}
