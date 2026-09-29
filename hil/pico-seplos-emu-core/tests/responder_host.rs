// Integration test binary (own crate root) — panic/expect are the correct failure mode for a
// test assertion; matches test-kit's own `#[allow(clippy::unwrap_used, clippy::expect_used,
// clippy::panic)]` convention for its test modules (see crates/test-kit/src/seplos/mod.rs).
#![allow(clippy::panic, clippy::expect_used)]

use lib_parse::seplos::Seplos;
use lib_parse::vendor::BmsVendorProtocol;
use pico_seplos_emu_core::seplos_responder::{BatteryState, SeplosResponder, SimReply};

#[test]
fn dip0_telemetry_request_round_trips_through_the_real_decoder() {
    let mut responder = SeplosResponder::new(BatteryState::sample_14_cell());
    let expected = BatteryState::sample_14_cell();

    // The exact DIP-0 telemetry request wire bytes (verified ground truth, do not alter):
    let request = b"~20004642F0010FD67\r";
    let reply = responder.respond(request);
    let bytes = match reply {
        SimReply::Bytes(b) => b,
        SimReply::Silence => panic!("expected a reply"),
    };

    assert_eq!(
        bytes.len(),
        160,
        "14-cell/6-temp telemetry reply must be 160 bytes on the wire"
    );

    let reading = Seplos::new()
        .parse_reading(&bytes, 0)
        .expect("sim-encoded telemetry decodes");
    assert_eq!(reading.device_address, 0);
    assert_eq!(
        reading.cell_voltages_mv.as_slice(),
        expected.cell_voltages_mv.as_slice()
    );
    assert_eq!(
        reading.temperatures_raw.as_slice(),
        expected.temperatures_raw.as_slice()
    );
    assert_eq!(reading.current_ca, expected.current_ca);
    assert_eq!(reading.total_voltage_cv, expected.total_voltage_cv);
    assert_eq!(
        reading.remaining_capacity_cah,
        expected.remaining_capacity_cah
    );
    assert_eq!(reading.typical_capacity_cah, expected.typical_capacity_cah);
    assert_eq!(reading.soc_dpct, expected.soc_dpct);
    assert_eq!(reading.rated_capacity_cah, expected.rated_capacity_cah);
    assert_eq!(reading.cycles, expected.cycles);
    assert_eq!(reading.soh_dpct, expected.soh_dpct);
    assert_eq!(reading.port_voltage_cv, expected.port_voltage_cv);
}

#[test]
fn unrecognized_command_gets_silence_not_a_fabricated_reply() {
    let mut responder = SeplosResponder::new(BatteryState::sample_14_cell());

    // A fault (0x44) request — CID2 this responder doesn't implement in phase 1 — must yield
    // SimReply::Silence, not a crash or a fabricated telemetry reply. Build the request with
    // lib_parse::seplos::command::build_command(0, lib_parse::seplos::command::Command::Fault,
    // &mut out) and assert responder.respond(&out) == SimReply::Silence.
    let mut out = heapless::Vec::<u8, 32>::new();
    lib_parse::seplos::command::build_command(
        0,
        lib_parse::seplos::command::Command::Fault,
        &mut out,
    )
    .expect("build fault command");

    assert_eq!(responder.respond(&out), SimReply::Silence);
}

#[test]
fn malformed_checksum_gets_silence() {
    let mut responder = SeplosResponder::new(BatteryState::sample_14_cell());

    // Corrupt one byte inside a well-formed telemetry request's INFO/checksum region and assert
    // the responder returns SimReply::Silence (never panics, never fabricates a reply for a
    // request it can't validate).
    let mut request = b"~20004642F0010FD67\r".to_vec();

    // Corrupt one byte inside the checksum region (last 4 hex chars before \r: "D67")
    // Flip a bit in the first checksum hex digit (D)
    let idx = request.len() - 4;
    request[idx] ^= 0x01;

    assert_eq!(responder.respond(&request), SimReply::Silence);
}
