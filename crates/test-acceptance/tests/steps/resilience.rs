//! Steps for the resilience feature area: Seplos fault-domain decoding, negative/hardening cases
//! (specific typed `SeplosError`/`ParseError` variants), and bounded-memory (OOM) behavior.
//!
//! GREP-GATE — NO re-implemented protocol/transform/checksum logic lives in this file. Every
//! decode / fault-map / diagnostics / codec operation delegates to the real, reused crates:
//! `lib_parse::seplos::{frame::ResponseFrame, fault::decode_fault, telemetry::decode_reading,
//! command::build_command}` and the `Seplos` `BmsVendorProtocol` impl (the real decoder);
//! `lib_parse::seplos::checksum::{frame_checksum, length_checksum}` (the single authoritative
//! frame-math impl, shared with the decoder and `SeplosSim`, NEVER re-derived);
//! `lib_mapping::transform::{ToDiagnostics, FromDiagnostics}` (the real fault<->DTC mapper);
//! `lib_mapping::codec::to_json_slice` (the real JSON wire codec); and
//! `test_kit::seplos::{SeplosSim, Behavior}` (the shared, checksum-reusing simulator).
//!
//! What lives here is Gherkin<->Rust plumbing only: assembling malformed test-vector frame bytes
//! (ASCII-hex formatting + the shared checksum fn — the same fixture pattern as the blessed
//! `lib-parse/tests/seplos_hardening.rs::make_response`), and asserting on the typed results.
//! Fault-flag name grammar and the error-variant-name helper are reused from `super::support::*`;
//! `dev()`/`now()` from `crate`.
//!
//! Test code only; freely uses unwrap/expect/panic (the workspace lint gate denies these in
//! production logic, not tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cucumber::{given, then, when};
use lib_io::transport::SerialTransport;

use heapless::Vec as HVec;
use lib_mapping::codec::to_json_slice;
use lib_mapping::schema::{
    ActiveDiagnosticState, DcMeasurementUpdate, DtcSeverity, FailureModeIndicator,
    PowerHardwareUpdate, MAX_DC_LINES, MAX_STORAGE_CELLS,
};
use lib_mapping::transform::{FromDiagnostics, ToDiagnostics, ToTms};
use lib_parse::canonical::{CanonicalFault, CanonicalReading, MAX_CELLS, MAX_TEMPS};
use lib_parse::seplos::checksum::{frame_checksum, length_checksum};
use lib_parse::seplos::command::{build_command, Command};
use lib_parse::seplos::fault::decode_fault;
use lib_parse::seplos::frame::ResponseFrame;
use lib_parse::seplos::telemetry::decode_reading;
use test_kit::seplos::{BatteryState, Behavior, SeplosSim, SimReply};

use crate::{dev, now, SpecWorld};

use super::support::{error_variant_name, get_fault_flag, set_fault_flag};

/// Scenario scratch for the resilience feature area. cucumber resets `SpecWorld` per scenario, so
/// every field starts `None`/empty and is populated only by the steps a given scenario runs.
#[derive(Default)]
pub struct ResilienceState {
    /// Raw response-frame bytes under test (crafted malformed vector or `SeplosSim` reply).
    frame: Vec<u8>,
    /// The `CanonicalFault` produced by `decode_fault` / constructed by name / recovered on decode.
    decoded_fault: Option<CanonicalFault>,
    /// The `CanonicalFault` recovered after a diagnostics codec + `FromDiagnostics` round-trip.
    round_trip_fault: Option<CanonicalFault>,
    /// The diagnostics payload produced by `ToDiagnostics` (or built for a decode scenario).
    diag: Option<ActiveDiagnosticState>,
    /// The `PowerHardwareUpdate` mapped from a reading (storage-cell cap scenario).
    hw: Option<PowerHardwareUpdate>,
    /// The `DcMeasurementUpdate` mapped from a reading (DC-line cap scenario).
    dc: Option<DcMeasurementUpdate>,
    /// Variant name of the most recent typed decode/parse/build error (`None` = success).
    error_variant: Option<String>,
    /// Result of a codec call into a deliberately-short buffer: `Ok(bytes_written)` or the
    /// `CodecError` variant name.
    codec_result: Option<Result<usize, String>>,
    /// A disconnected host serial transport under test (for serial resilience scenarios).
    disconnected_transport: Option<lib_io::host::HostSerial>,
    /// Result of a transport operation (read/write/set_direction): `Ok(())` or an error string.
    transport_result: Option<Result<(), String>>,
}

impl std::fmt::Debug for ResilienceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResilienceState")
            .field("frame", &self.frame)
            .field("decoded_fault", &self.decoded_fault)
            .field("round_trip_fault", &self.round_trip_fault)
            .field("diag", &self.diag)
            .field("hw", &self.hw)
            .field("dc", &self.dc)
            .field("error_variant", &self.error_variant)
            .field("codec_result", &self.codec_result)
            .field("disconnected_transport", &"<HostSerial>")
            .field("transport_result", &self.transport_result)
            .finish()
    }
}

// ---- test-vector fixture builders (frame-math delegated to the real checksum crate) ------------

/// Push `byte` as two uppercase ASCII-hex chars. ASCII formatting only — not checksum/protocol
/// logic (mirrors `seplos_hardening.rs::push_hex_u8`).
fn push_hex_u8(out: &mut Vec<u8>, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push(HEX[(byte >> 4) as usize]);
    out.push(HEX[(byte & 0xF) as usize]);
}

/// Assemble a Seplos response frame with a VALID outer checksum (so a single overridden field
/// isolates exactly one failure mode). Byte-for-byte the same fixture builder as the blessed
/// `lib-parse/tests/seplos_hardening.rs::make_response` — the checksum/LCHKSUM math is the
/// real `lib_parse::seplos::checksum` impl, never re-derived here.
fn make_response(
    adr: u8,
    rtn: u8,
    info: &[u8],
    lchksum: Option<u8>,
    lenid: Option<u16>,
) -> Vec<u8> {
    let info_hex_len = (info.len() * 2) as u16;
    let lenid = lenid.unwrap_or(info_hex_len);
    let lchksum = lchksum.unwrap_or_else(|| length_checksum(lenid));
    let length: u16 = ((lchksum as u16) << 12) | (lenid & 0x0FFF);

    let mut body = Vec::new(); // VER..INFO (the checksum-covered region, ASCII)
    body.extend_from_slice(b"20");
    push_hex_u8(&mut body, adr);
    body.extend_from_slice(b"46");
    push_hex_u8(&mut body, rtn);
    push_hex_u8(&mut body, (length >> 8) as u8);
    push_hex_u8(&mut body, (length & 0xFF) as u8);
    for &b in info {
        push_hex_u8(&mut body, b);
    }

    let cs = frame_checksum(&body);
    let mut frame = Vec::new();
    frame.push(b'~');
    frame.extend_from_slice(&body);
    push_hex_u8(&mut frame, (cs >> 8) as u8);
    push_hex_u8(&mut frame, (cs & 0xFF) as u8);
    frame.push(b'\r');
    frame
}

/// Build a `0x44`-shaped fault INFO payload of `len` bytes with a single `(byte, bit)` set, wrapped
/// in a valid frame. `len` must cover the byte under test.
fn fault_frame_single_bit(byte: usize, bit: u8, len: usize) -> Vec<u8> {
    let mut info = vec![0u8; len];
    info[byte] |= 1 << bit;
    make_response(0, 0, &info, None, None)
}

/// A malformed test vector named by `defect`, each engineered to trip exactly one `SeplosError`
/// variant in `ResponseFrame::parse` (check order per `frame.rs`). Fixture assembly only.
fn crafted_frame(defect: &str) -> Vec<u8> {
    match defect {
        // No leading SOI '~'.
        "bad_soi" => b"20004600GARBAGE\r".to_vec(),
        // A checksum hex char corrupted to a non-hex byte: the checksum field is hex-decoded
        // (Step 4) before the value comparison, so this surfaces NonHex, not BadChecksum.
        "non_hex" => {
            let mut f = make_response(0, 0, &[0xAB, 0xCD], None, None);
            let cs_pos = f.len() - 3; // one of the 4 checksum chars (…C C C C \r)
            f[cs_pos] = b'G';
            f
        }
        // Correct LENID, wrong LCHKSUM nibble; outer checksum still valid.
        "bad_length_checksum" => {
            let good = length_checksum(4);
            let bad = (good ^ 0x1) & 0xF;
            make_response(0, 0, &[0xAB, 0xCD], Some(bad), None)
        }
        // LENID declares a length that does not match the actual 2-byte INFO; LCHKSUM matches it so
        // the check falls through to the length comparison.
        "length_mismatch" => make_response(0, 0, &[0xAB, 0xCD], None, Some(6)),
        // Non-zero RTN — a device-reported error return code.
        "rtn_error" => make_response(0, 0x02, &[0xAB, 0xCD], None, None),
        // 300-byte INFO overruns the 256-byte info Vec cap → TooManyElements (no panic).
        "too_many_elements" => make_response(0, 0, &vec![0x5Au8; 300], None, None),
        other => panic!("unknown crafted-frame defect {other:?} in feature file"),
    }
}

/// A `CanonicalReading` fully populated to the 16-cell / 8-temp canonical caps — a maximal input
/// for the bounded-memory mapping scenarios. Literal fixture data, no transform logic.
fn full_reading() -> CanonicalReading {
    let mut cells: HVec<u16, MAX_CELLS> = HVec::new();
    for i in 0..MAX_CELLS as u16 {
        cells.push(3800 + i).expect("cell fits MAX_CELLS");
    }
    let mut temps: HVec<u16, MAX_TEMPS> = HVec::new();
    for i in 0..MAX_TEMPS as u16 {
        temps.push(2981 + i).expect("temp fits MAX_TEMPS");
    }
    CanonicalReading {
        device_address: 0,
        cell_voltages_mv: cells,
        temperatures_raw: temps,
        current_ca: -144,
        total_voltage_cv: 5451,
        remaining_capacity_cah: 13953,
        typical_capacity_cah: 18000,
        rated_capacity_cah: 18000,
        soc_dpct: 775,
        soh_dpct: 1000,
        cycles: 2,
        port_voltage_cv: 5453,
    }
}

fn a_fault(name: &str) -> CanonicalFault {
    let mut f = CanonicalFault {
        device_address: 0,
        ..Default::default()
    };
    set_fault_flag(&mut f, name);
    f
}

// ---- Given: fixtures ---------------------------------------------------------------------------

#[given(expr = "a synthetic Seplos fault frame with INFO byte {int} bit {int} set")]
fn given_fault_frame_single_bit(world: &mut SpecWorld, byte: i64, bit: i64) {
    // 11 bytes covers the whole (byte 3..=10) fault map with room for byte 10.
    world.resilience.frame = fault_frame_single_bit(byte as usize, bit as u8, 11);
}

#[given(expr = "a synthetic Seplos fault frame with only {int} INFO bytes")]
fn given_fault_frame_short(world: &mut SpecWorld, len: i64) {
    world.resilience.frame = make_response(0, 0, &vec![0u8; len as usize], None, None);
}

#[given(expr = "the fault has flag {word} set")]
fn given_fault_flag_set(world: &mut SpecWorld, name: String) {
    world.resilience.decoded_fault = Some(a_fault(&name));
}

#[given("the fault has no flags set")]
fn given_fault_no_flags(world: &mut SpecWorld) {
    world.resilience.decoded_fault = Some(CanonicalFault {
        device_address: 0,
        ..Default::default()
    });
}

#[given(expr = "a diagnostics payload carrying only suspect parameter {int}")]
fn given_diag_with_spn(world: &mut SpecWorld, spn: i64) {
    // Build a real single-DTC payload via the mapper, then relabel its suspect parameter to the
    // (possibly unrecognised) SPN under test — no hand-built DTC struct, no invented codec.
    let mut diag = a_fault("high_voltage_protection").to_active_diagnostics(dev(), now());
    diag.codes[0].suspect_parameter = spn as u32;
    world.resilience.diag = Some(diag);
}

#[given("the 14-cell sample battery under test")]
fn given_sim_battery(world: &mut SpecWorld) {
    let state = BatteryState::sample_14_cell();
    world.sim = Some(SeplosSim::new(state.clone()));
    world.state = Some(state);
}

#[given(expr = "a Seplos telemetry frame declaring {int} cells")]
fn given_telemetry_declaring_cells(world: &mut SpecWorld, cells: i64) {
    // INFO = [data_flag, command_group, num_cells]; a num_cells over MAX_CELLS trips TooManyElements.
    let info = [0x00u8, 0x00, cells as u8];
    world.resilience.frame = make_response(0, 0, &info, None, None);
}

#[given(expr = "a Seplos telemetry frame declaring {int} cell and {int} temperature sensors")]
#[given(expr = "a Seplos telemetry frame declaring {int} cells and {int} temperature sensors")]
fn given_telemetry_declaring_cells_temps(world: &mut SpecWorld, cells: i64, temps: i64) {
    // [flag, group, num_cells, <cells*2 bytes>, num_temps]; a num_temps over MAX_TEMPS trips it.
    let mut info = vec![0x00u8, 0x00, cells as u8];
    info.resize(info.len() + cells as usize * 2, 0u8);
    info.push(temps as u8);
    world.resilience.frame = make_response(0, 0, &info, None, None);
}

#[given(expr = "a Seplos telemetry frame with only {int} INFO bytes")]
fn given_telemetry_short_info(world: &mut SpecWorld, len: i64) {
    world.resilience.frame = make_response(0, 0, &vec![0u8; len as usize], None, None);
}

#[given("an all-faults diagnostics payload")]
fn given_all_faults_diag(world: &mut SpecWorld) {
    let mut f = CanonicalFault {
        device_address: 0,
        ..Default::default()
    };
    for name in FLAG_NAMES {
        set_fault_flag(&mut f, name);
    }
    world.resilience.diag = Some(f.to_active_diagnostics(dev(), now()));
}

#[given("a maximally populated 16-cell canonical reading")]
fn given_full_reading(world: &mut SpecWorld) {
    world.reading = Some(full_reading());
}

#[given(expr = "a crafted Seplos frame with defect {word}")]
fn given_crafted_defect(world: &mut SpecWorld, defect: String) {
    world.resilience.frame = crafted_frame(&defect);
}

#[given(expr = "a pathological input named {word}")]
fn given_pathological(world: &mut SpecWorld, kind: String) {
    world.resilience.frame = match kind.as_str() {
        "empty" => Vec::new(),
        "soi_only" => b"~".to_vec(),
        "all_zeros" => vec![0u8; 64],
        "oversized" => vec![0x5Au8; 4096],
        "nonframe_text" => b"the quick brown fox\r".to_vec(),
        other => panic!("unknown pathological input {other:?} in feature file"),
    };
}

// ---- When: actions (all delegate to the real decoder / mapper / codec) -------------------------

#[when("the fault frame is decoded")]
fn when_decode_fault_frame(world: &mut SpecWorld) {
    match ResponseFrame::parse(&world.resilience.frame) {
        Ok(frame) => match decode_fault(&frame) {
            Ok(f) => {
                world.resilience.decoded_fault = Some(f);
                world.resilience.error_variant = None;
            }
            Err(e) => world.resilience.error_variant = Some(error_variant_name(&e)),
        },
        Err(e) => world.resilience.error_variant = Some(error_variant_name(&e)),
    }
}

#[when("the fault is mapped to diagnostics")]
fn when_map_fault_diag(world: &mut SpecWorld) {
    let f = world.resilience.decoded_fault.expect("fault set");
    world.resilience.diag = Some(f.to_active_diagnostics(dev(), now()));
}

#[when("the fault round-trips through the diagnostics JSON codec")]
fn when_fault_round_trip(world: &mut SpecWorld) {
    let f = world.resilience.decoded_fault.expect("fault set");
    let diag = f.to_active_diagnostics(dev(), now());
    let mut buf = [0u8; 4096];
    let n = to_json_slice(&diag, &mut buf).expect("encode diagnostics");
    let (decoded, _): (ActiveDiagnosticState, usize) =
        serde_json_core::from_slice(&buf[..n]).expect("decode diagnostics");
    world.resilience.round_trip_fault = Some(CanonicalFault::from_active_diagnostics(&decoded));
}

#[when("the diagnostics payload is decoded to canonical faults")]
fn when_decode_diag(world: &mut SpecWorld) {
    let diag = world.resilience.diag.as_ref().expect("diagnostics set");
    world.resilience.decoded_fault = Some(CanonicalFault::from_active_diagnostics(diag));
}

#[when("the telemetry frame is decoded")]
fn when_decode_telemetry(world: &mut SpecWorld) {
    match ResponseFrame::parse(&world.resilience.frame) {
        Ok(frame) => match decode_reading(&frame) {
            Ok(r) => {
                world.reading = Some(r);
                world.resilience.error_variant = None;
            }
            Err(e) => world.resilience.error_variant = Some(error_variant_name(&e)),
        },
        Err(e) => world.resilience.error_variant = Some(error_variant_name(&e)),
    }
}

#[when(expr = "a Seplos command is built for DIP {int}")]
fn when_build_command_for_dip(world: &mut SpecWorld, dip: i64) {
    let mut out: HVec<u8, 32> = HVec::new();
    match build_command(dip as u8, Command::Telemetry, &mut out) {
        Ok(()) => world.resilience.error_variant = None,
        Err(e) => world.resilience.error_variant = Some(error_variant_name(&e)),
    }
}

#[when(expr = "the battery replies with behavior {word} and the reply is parsed")]
fn when_behavior_reply(world: &mut SpecWorld, behavior: String) {
    let (b, dip) = match behavior.as_str() {
        "CorruptChecksum" => (Behavior::CorruptChecksum, 0u8),
        "WrongAddress" => (Behavior::WrongAddress, 0),
        "Truncated" => (Behavior::Truncated(10), 0),
        "Silence" => (Behavior::Silence, 0),
        "Garbage" => (Behavior::Garbage, 0),
        other => panic!("unknown behavior {other:?} in feature file"),
    };
    let sim = world.sim.as_mut().expect("simulator initialized");
    sim.set_behavior(b);
    let mut req: HVec<u8, 32> = HVec::new();
    build_command(dip, Command::Telemetry, &mut req).expect("build telemetry request");
    let reply = sim.respond(&req);
    world.resilience.error_variant = Some(match reply {
        // A silent bus is a distinct, first-class outcome — not an error frame at all.
        SimReply::Silence => "Silence".to_string(),
        SimReply::Bytes(bytes) if bytes.is_empty() => "Silence".to_string(),
        SimReply::Bytes(bytes) => match ResponseFrame::parse(bytes.as_slice()) {
            Err(e) => error_variant_name(&e),
            // A well-formed frame for the WRONG DIP parses fine at the frame layer; the vendor
            // layer is where the address mismatch surfaces.
            Ok(frame) if frame.adr != dip => "AddressMismatch".to_string(),
            Ok(_) => "Ok".to_string(),
        },
    });
}

#[when("the reading is mapped to resilient power hardware")]
fn when_map_hw_res(world: &mut SpecWorld) {
    let reading = world.reading.clone().expect("reading set");
    world.resilience.hw = Some(reading.to_power_hardware(dev(), now()));
}

#[when("the reading is mapped to a resilient DC measurement")]
fn when_map_dc_res(world: &mut SpecWorld) {
    let reading = world.reading.clone().expect("reading set");
    world.resilience.dc = Some(reading.to_dc_measurement(dev(), now()));
}

#[when(expr = "the diagnostics are JSON-encoded into a {int}-byte buffer")]
fn when_json_encode_short(world: &mut SpecWorld, n: i64) {
    let diag = world.resilience.diag.as_ref().expect("diagnostics set");
    let mut buf = vec![0u8; n as usize];
    world.resilience.codec_result =
        Some(to_json_slice(diag, &mut buf).map_err(|e| error_variant_name(&e)));
}

#[when("the input is parsed as a frame")]
fn when_parse_arbitrary(world: &mut SpecWorld) {
    // Totality probe: the parser must return a Result (never panic) on any input. Reaching the
    // Then step at all is the proof; we also record the outcome so the Then can assert on it.
    world.resilience.error_variant = match ResponseFrame::parse(&world.resilience.frame) {
        Ok(_) => None,
        Err(e) => Some(error_variant_name(&e)),
    };
}

// ---- Then: assertions --------------------------------------------------------------------------

#[then(expr = "the decoded fault sets only the {word} flag")]
fn then_only_flag(world: &mut SpecWorld, name: String) {
    let f = world.resilience.decoded_fault.expect("fault decoded");
    assert!(get_fault_flag(&f, &name), "expected {name} to be set");
    for other in FLAG_NAMES {
        if other != name {
            assert!(
                !get_fault_flag(&f, other),
                "unexpected flag {other} also set"
            );
        }
    }
    assert!(f.any(), "any() must be true when a flag is set");
}

#[then("the decoded fault reports any as true")]
fn then_any_true(world: &mut SpecWorld) {
    assert!(world.resilience.decoded_fault.expect("fault set").any());
}

#[then("the decoded fault reports any as false")]
fn then_any_false(world: &mut SpecWorld) {
    assert!(!world.resilience.decoded_fault.expect("fault set").any());
}

#[then("the fault decode succeeds")]
fn then_fault_decode_ok(world: &mut SpecWorld) {
    assert!(
        world.resilience.error_variant.is_none(),
        "expected a clean decode, got error {:?}",
        world.resilience.error_variant
    );
    assert!(world.resilience.decoded_fault.is_some(), "no fault decoded");
}

#[then("there is exactly one diagnostic code")]
fn then_one_code(world: &mut SpecWorld) {
    let diag = world.resilience.diag.as_ref().expect("diagnostics set");
    assert_eq!(diag.codes.len(), 1, "expected exactly one DTC");
    assert_eq!(diag.overflow, 0, "no overflow expected for a single flag");
}

#[then(expr = "the diagnostic code has suspect parameter {int}")]
fn then_code_spn(world: &mut SpecWorld, spn: i64) {
    let diag = world.resilience.diag.as_ref().expect("diagnostics set");
    assert_eq!(diag.codes[0].suspect_parameter, spn as u32);
}

#[then(expr = "the diagnostic code hint is {string}")]
fn then_code_hint(world: &mut SpecWorld, hint: String) {
    let diag = world.resilience.diag.as_ref().expect("diagnostics set");
    assert_eq!(diag.codes[0].hint.as_str(), hint);
}

#[then("the diagnostic code is a Major ConditionExists fault occurring once")]
fn then_code_shape(world: &mut SpecWorld) {
    let code = &world
        .resilience
        .diag
        .as_ref()
        .expect("diagnostics set")
        .codes[0];
    assert_eq!(code.failure_mode, FailureModeIndicator::ConditionExists);
    assert_eq!(code.severity, DtcSeverity::Major);
    assert_eq!(code.occurrence_count, 1);
    assert!(
        code.port_number.is_none()
            && code.estimate_time.is_none()
            && code.estimate_severity.is_none(),
        "the Seplos mapper emits no optional DTC fields"
    );
}

#[then(expr = "the round-tripped fault sets only the {word} flag")]
fn then_round_trip_flag(world: &mut SpecWorld, name: String) {
    let f = world.resilience.round_trip_fault.expect("round-trip fault");
    assert!(get_fault_flag(&f, &name), "expected {name} recovered");
    for other in FLAG_NAMES {
        if other != name {
            assert!(
                !get_fault_flag(&f, other),
                "unexpected flag {other} recovered"
            );
        }
    }
}

#[then(expr = "the parse is rejected with error {word}")]
#[then(expr = "the decode is rejected with error {word}")]
#[then(expr = "the build is rejected with error {word}")]
#[then(expr = "the reply resolves to outcome {word}")]
fn then_error_variant(world: &mut SpecWorld, expected: String) {
    assert_eq!(
        world.resilience.error_variant.as_deref(),
        Some(expected.as_str()),
        "wrong outcome variant"
    );
}

#[then("the parser returns without panicking")]
fn then_no_panic(world: &mut SpecWorld) {
    // Reaching this step proves the parser did not panic on the pathological input; the recorded
    // outcome is either a clean parse or a typed error, never an abort.
    let _ = &world.resilience.error_variant;
}

#[then(expr = "the storage-cell count is within the {int}-cell cap")]
fn then_storage_within_cap(world: &mut SpecWorld, cap: i64) {
    assert_eq!(
        cap as usize, MAX_STORAGE_CELLS,
        "cap literal must match the schema const"
    );
    let hw = world.resilience.hw.as_ref().expect("hw mapped");
    let es = hw.energy_storage.as_ref().expect("energy_storage present");
    assert!(
        es.storage_cells.len() <= MAX_STORAGE_CELLS,
        "storage cells {} exceed the {MAX_STORAGE_CELLS}-cell cap",
        es.storage_cells.len()
    );
}

#[then(expr = "the DC line count is within the {int}-line cap")]
fn then_dc_within_cap(world: &mut SpecWorld, cap: i64) {
    assert_eq!(
        cap as usize, MAX_DC_LINES,
        "cap literal must match the schema const"
    );
    let dc = world.resilience.dc.as_ref().expect("dc mapped");
    for port in &dc.external_measurement {
        assert!(
            port.line.len() <= MAX_DC_LINES,
            "DC lines {} exceed the {MAX_DC_LINES}-line cap",
            port.line.len()
        );
    }
}

#[then(expr = "the codec fails with error {word}")]
fn then_codec_fails(world: &mut SpecWorld, expected: String) {
    match world.resilience.codec_result.as_ref().expect("codec ran") {
        Err(v) => assert_eq!(v, &expected, "wrong codec error variant"),
        Ok(n) => panic!("expected codec error {expected}, got Ok({n})"),
    }
}

// ---- Serial transport resilience: disconnected-transport contract pinning -------------------------

#[given("a disconnected host serial transport")]
async fn given_disconnected_serial(world: &mut SpecWorld) {
    world.resilience.disconnected_transport = Some(lib_io::host::HostSerial::disconnected());
    world.resilience.transport_result = None;
}

#[when("a read is attempted on the disconnected transport")]
async fn when_read_disconnected(world: &mut SpecWorld) {
    let mut transport = world
        .resilience
        .disconnected_transport
        .take()
        .expect("transport initialized");
    let mut buf = [0u8; 64];
    let result: Result<(), String> = transport
        .read(&mut buf)
        .await
        .map(|_| ())
        .map_err(|e| format!("{e:?}"));
    world.resilience.transport_result = Some(result);
    world.resilience.disconnected_transport = Some(transport);
}

#[when("a write is attempted on the disconnected transport")]
async fn when_write_disconnected(world: &mut SpecWorld) {
    let mut transport = world
        .resilience
        .disconnected_transport
        .take()
        .expect("transport initialized");
    let result: Result<(), String> = transport
        .write_all(b"x")
        .await
        .map_err(|e| format!("{e:?}"));
    world.resilience.transport_result = Some(result);
    world.resilience.disconnected_transport = Some(transport);
}

#[when("the transport direction is set")]
async fn when_set_direction(world: &mut SpecWorld) {
    let mut transport = world
        .resilience
        .disconnected_transport
        .take()
        .expect("transport initialized");
    let result: Result<(), String> = transport
        .set_direction(lib_io::transport::Direction::Transmit)
        .await
        .map_err(|e| format!("{e:?}"));
    world.resilience.transport_result = Some(result);
    world.resilience.disconnected_transport = Some(transport);
}

#[then("the read returns an IoError timeout without panicking")]
fn then_read_timeout(world: &mut SpecWorld) {
    match world
        .resilience
        .transport_result
        .as_ref()
        .expect("read executed")
    {
        Err(e) if e.contains("Timeout") => {
            // Success: the timeout is the expected outcome
        }
        other => panic!("expected Timeout error, got {:?}", other),
    }
}

#[then("the write returns an IoError timeout without panicking")]
fn then_write_timeout(world: &mut SpecWorld) {
    match world
        .resilience
        .transport_result
        .as_ref()
        .expect("write executed")
    {
        Err(e) if e.contains("Timeout") => {
            // Success: the timeout is the expected outcome
        }
        other => panic!("expected Timeout error, got {:?}", other),
    }
}

#[then("the direction change succeeds")]
fn then_direction_succeeds(world: &mut SpecWorld) {
    match world
        .resilience
        .transport_result
        .as_ref()
        .expect("set_direction executed")
    {
        Ok(()) => {
            // Success: the direction change is a benign no-op
        }
        other => panic!("expected Ok(()), got {:?}", other),
    }
}

/// The 16 `CanonicalFault` flag names in `FAULT_TABLE` / decode order. Used only to sweep "no other
/// flag is set" assertions — the authoritative getter/setter grammar lives in `super::support`.
const FLAG_NAMES: [&str; 16] = [
    "high_voltage_protection",
    "low_voltage_protection",
    "high_temperature_charge_protection",
    "low_temperature_charge_protection",
    "charge_overcurrent_protection",
    "discharge_overcurrent_protection",
    "short_circuit_protection",
    "mosfet_fault",
    "cell_balance_warning",
    "low_capacity_warning",
    "high_temperature_discharge_warning",
    "low_temperature_discharge_warning",
    "low_voltage_warning",
    "high_voltage_warning",
    "discharge_current_warning",
    "charge_current_warning",
];
