//! `tcp-sim` feature acceptance test: feed the verbatim Seplos DIP0 request vectors straight into
//! `SeplosSim::respond` (the same pure core `seplos-tcp-sim` wraps) and confirm the reply, parsed
//! back by `lib_parse`, yields `BatteryState::sample_14_cell()`'s canonical values.
//!
//! Request vectors are byte-exact, copied from `crates/lib-parse/tests/seplos_vectors.rs`
//! (`command_frames_are_byte_exact`) — never re-derived here.

#![cfg(feature = "tcp-sim")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_parse::seplos::Seplos;
use lib_parse::vendor::BmsVendorProtocol;
use test_kit::seplos::{BatteryState, FaultFlags, SeplosSim, SimReply};

/// Verbatim DIP0 `0x42` telemetry request (`lib-parse/tests/seplos_vectors.rs`).
const REQUEST_0X42_DIP0: &[u8] = b"~20004642F0010FD67\r";
/// Verbatim DIP0 `0x44` fault request (`lib-parse/tests/seplos_vectors.rs`).
const REQUEST_0X44_DIP0: &[u8] = b"~20004644F0010FD65\r";

#[test]
fn dip0_telemetry_request_yields_sample_14_cell_canonical_values() {
    let mut sim = SeplosSim::new(BatteryState::sample_14_cell());

    let reply = sim.respond(REQUEST_0X42_DIP0);
    let bytes = match reply {
        SimReply::Bytes(b) => b,
        SimReply::Silence => panic!("expected a reply for a well-formed 0x42 DIP0 request"),
    };

    let reading = Seplos::new()
        .parse_reading(&bytes, 0)
        .expect("sim-encoded 0x42 reply decodes");

    assert_eq!(reading.device_address, 0);
    assert_eq!(reading.cell_voltages_mv.len(), 14);
    assert_eq!(reading.total_voltage_cv, 5329); // sum(cells)/10 = 53291/10, see sample_14_cell
    assert_eq!(reading.current_ca, -164);
    assert_eq!(reading.soc_dpct, 775);
    assert_eq!(reading.cycles, 2);
    assert_eq!(reading.soh_dpct, 1000);
}

#[test]
fn dip0_fault_request_yields_the_four_expected_flags() {
    let mut state = BatteryState::sample_14_cell();
    state.faults = FaultFlags {
        high_voltage_protection: true,
        short_circuit_protection: true,
        low_capacity_warning: true,
        charge_current_warning: true,
        ..Default::default()
    };
    let mut sim = SeplosSim::new(state);

    let reply = sim.respond(REQUEST_0X44_DIP0);
    let bytes = match reply {
        SimReply::Bytes(b) => b,
        SimReply::Silence => panic!("expected a reply for a well-formed 0x44 DIP0 request"),
    };

    let fault = Seplos::new()
        .parse_fault(&bytes, 0)
        .expect("sim-encoded 0x44 reply decodes");

    assert!(fault.high_voltage_protection);
    assert!(fault.short_circuit_protection);
    assert!(fault.low_capacity_warning);
    assert!(fault.charge_current_warning);

    // Neighbours that must stay clear, guarding against a bit/byte mixup.
    assert!(!fault.low_voltage_protection);
    assert!(!fault.discharge_overcurrent_protection);
    assert!(!fault.cell_balance_warning);
    assert!(!fault.discharge_current_warning);
}
