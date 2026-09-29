//! Runtime `BatteryState` spec-grammar acceptance tests (#226 stage W1): grammar round-trips,
//! every fail-closed arm, `MAX_CELLS`/`MAX_TEMPS` bounds, override-vs-derived `total_voltage_cv`,
//! the programmatic builder, and byte-exact backward compatibility of the pre-existing
//! `sample14` state (the accuracy fixtures' encoded frame must not move under this refactor).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_parse::seplos::command::{build_command, Command};
use lib_parse::seplos::Seplos;
use lib_parse::vendor::BmsVendorProtocol;
use test_kit::seplos::{
    parse_battery_state_spec, BatteryState, BatteryStateSpec, SeplosSim, SimReply, MAX_CELLS,
    MAX_TEMPS,
};

fn request(dip: u8, cmd: Command) -> heapless::Vec<u8, 32> {
    let mut out = heapless::Vec::new();
    build_command(dip, cmd, &mut out).expect("build request");
    out
}

fn encode(state: BatteryState) -> std::vec::Vec<u8> {
    let mut sim = SeplosSim::new(state);
    let req = request(0, Command::Telemetry);
    match sim.respond(&req) {
        SimReply::Bytes(b) => b.as_slice().to_vec(),
        SimReply::Silence => panic!("expected a reply"),
    }
}

// ---------------------------------------------------------------------------------------------
// Backward compatibility: sample14 must be byte-identical after the spec-grammar refactor.
// ---------------------------------------------------------------------------------------------

#[test]
fn sample14_encoded_frame_is_byte_identical_after_the_spec_refactor() {
    let bytes = encode(BatteryState::sample_14_cell());
    assert_eq!(
        &bytes[..],
        &b"~20004600A08E00000E0ED80ED90EDA0EDB0EDC0EDD0EDE0EDF0EE00EE10EE20EE30EE40EE5060BA50BA70BA90BAB0BAD0BAFFF5C14D105730A070803070708000203E8154D0000000000000000DDE9\r"[..],
        "the sample14 telemetry reply must be byte-identical to before this refactor"
    );
}

// ---------------------------------------------------------------------------------------------
// Grammar round-trip: spec -> BatteryState -> encoded frame -> lib_parse decode -> values match.
// ---------------------------------------------------------------------------------------------

#[test]
fn grammar_round_trip_decodes_back_to_the_same_values() {
    let spec = "cells=3111,3222,3333,3444 temps=2951,2961,2971 current=-500 soc=612 soh=990 \
                cycles=7 remaining=1234 typical=1500 rated=1600 port=5400";
    let state = parse_battery_state_spec(spec).expect("valid spec parses");
    let expected = state.clone();
    let bytes = encode(state);

    let reading = Seplos::new()
        .parse_reading(&bytes, 0)
        .expect("sim-encoded telemetry decodes");
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
fn unspecified_keys_default_from_the_sample_14_cell_baseline() {
    let state = parse_battery_state_spec("current=-999").expect("valid spec parses");
    let baseline = BatteryState::sample_14_cell();
    assert_eq!(state.current_ca, -999);
    assert_eq!(state.cell_voltages_mv, baseline.cell_voltages_mv);
    assert_eq!(state.temperatures_raw, baseline.temperatures_raw);
    assert_eq!(state.soc_dpct, baseline.soc_dpct);
    assert_eq!(state.soh_dpct, baseline.soh_dpct);
    assert_eq!(state.cycles, baseline.cycles);
    assert_eq!(state.serial, baseline.serial);
    assert_eq!(state.faults, baseline.faults);
}

// ---------------------------------------------------------------------------------------------
// total_voltage_cv: derived by default, explicitly overridable for negative tests.
// ---------------------------------------------------------------------------------------------

#[test]
fn total_voltage_defaults_to_the_derived_sum_of_cells() {
    let state = parse_battery_state_spec("cells=1000,2000,3000").expect("valid spec parses");
    assert_eq!(state.total_voltage_cv, 600); // (1000+2000+3000)/10
}

#[test]
fn total_voltage_explicit_override_can_be_deliberately_inconsistent() {
    let state =
        parse_battery_state_spec("cells=1000,2000,3000 total=1").expect("valid spec parses");
    assert_eq!(
        state.total_voltage_cv, 1,
        "an explicit total= override must win over the derived sum, even when inconsistent"
    );
}

// ---------------------------------------------------------------------------------------------
// Bounds: exactly MAX_CELLS/MAX_TEMPS accepted, one more rejected.
// ---------------------------------------------------------------------------------------------

fn csv_list(n: usize, start: u16) -> std::string::String {
    (0..n)
        .map(|i| (start + i as u16).to_string())
        .collect::<std::vec::Vec<_>>()
        .join(",")
}

#[test]
fn exactly_max_cells_is_accepted() {
    let spec = std::format!("cells={}", csv_list(MAX_CELLS, 3000));
    let state = parse_battery_state_spec(&spec).expect("exactly MAX_CELLS must be accepted");
    assert_eq!(state.cell_voltages_mv.len(), MAX_CELLS);
}

#[test]
fn max_cells_plus_one_is_rejected() {
    let spec = std::format!("cells={}", csv_list(MAX_CELLS + 1, 3000));
    let err = parse_battery_state_spec(&spec).expect_err("MAX_CELLS + 1 must be rejected");
    assert!(
        err.contains("too many cells"),
        "error must name the cells-count violation, got: {err}"
    );
}

#[test]
fn exactly_max_temps_is_accepted() {
    let spec = std::format!("temps={}", csv_list(MAX_TEMPS, 2900));
    let state = parse_battery_state_spec(&spec).expect("exactly MAX_TEMPS must be accepted");
    assert_eq!(state.temperatures_raw.len(), MAX_TEMPS);
}

#[test]
fn max_temps_plus_one_is_rejected() {
    let spec = std::format!("temps={}", csv_list(MAX_TEMPS + 1, 2900));
    let err = parse_battery_state_spec(&spec).expect_err("MAX_TEMPS + 1 must be rejected");
    assert!(
        err.contains("too many temps"),
        "error must name the temps-count violation, got: {err}"
    );
}

// ---------------------------------------------------------------------------------------------
// Fail-closed arms: never panic, always a precise Err.
// ---------------------------------------------------------------------------------------------

#[test]
fn unknown_key_is_rejected() {
    let err = parse_battery_state_spec("bogus=1").expect_err("unknown key must be rejected");
    assert!(err.contains("unknown battery-state spec key"), "{err}");
}

#[test]
fn non_key_value_token_is_rejected() {
    let err =
        parse_battery_state_spec("not-a-pair").expect_err("non key=value token must be rejected");
    assert!(err.contains("key=value"), "{err}");
}

#[test]
fn non_numeric_value_is_rejected() {
    let err =
        parse_battery_state_spec("current=abc").expect_err("non-numeric value must be rejected");
    assert!(err.contains("invalid number"), "{err}");
}

#[test]
fn empty_cells_list_is_rejected() {
    let err = parse_battery_state_spec("cells=").expect_err("empty cells list must be rejected");
    assert!(err.contains("cells"), "{err}");
}

#[test]
fn empty_temps_list_is_rejected() {
    let err = parse_battery_state_spec("temps=").expect_err("empty temps list must be rejected");
    assert!(err.contains("temps"), "{err}");
}

#[test]
fn non_numeric_cell_entry_is_rejected() {
    let err = parse_battery_state_spec("cells=3800,oops,3810")
        .expect_err("non-numeric cell entry must be rejected");
    assert!(err.contains("invalid value"), "{err}");
}

// ---------------------------------------------------------------------------------------------
// Programmatic builder: Rust callers can construct states without the textual grammar, and it
// agrees exactly with the text-grammar path for the same values.
// ---------------------------------------------------------------------------------------------

#[test]
fn programmatic_builder_matches_the_equivalent_text_spec() {
    let via_builder = BatteryStateSpec::new()
        .cells(&[3700, 3710, 3720])
        .expect("valid cells")
        .current_ca(-42)
        .soc_dpct(500)
        .build()
        .expect("builder produces a state");

    let via_text = parse_battery_state_spec("cells=3700,3710,3720 current=-42 soc=500")
        .expect("text spec parses");

    assert_eq!(via_builder, via_text);
}

#[test]
fn programmatic_builder_rejects_empty_cells() {
    assert!(BatteryStateSpec::new().cells(&[]).is_err());
}

#[test]
fn programmatic_builder_rejects_over_max_cells() {
    let too_many: std::vec::Vec<u16> = (0..(MAX_CELLS as u16 + 1)).map(|i| 3000 + i).collect();
    assert!(BatteryStateSpec::new().cells(&too_many).is_err());
}
