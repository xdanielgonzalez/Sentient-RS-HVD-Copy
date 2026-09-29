//! Voltronic/PI30 field-decode acceptance vectors: `QPIGS`/`QMOD`/`QPIWS`/`QPIRI`/`QID` payloads
//! (already stripped of the leading `(`, CRC bytes, and trailing `\r` — that framing is
//! `Voltronic::validate_frame`'s job, covered in `voltronic_vectors.rs`).
//!
//! SYNTHETIC: built per the protocol facts, not observed on the wire (decision 0001's open risk —
//! no real Voltronic/QPIGS wire captures exist yet).
#![cfg(feature = "voltronic")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_parse::canonical::InverterMode;
use lib_parse::voltronic::fields::{
    parse_fault, parse_mode, parse_ratings, parse_serial, parse_status,
};

// ---- QPIGS: golden 21-field status payload ----

const GOLDEN_QPIGS: &str = "230.0 50.0 230.0 50.0 1000 59 00 380 51.20 010 000 035 005.0 185.0 00000 000 10010110 000 00.0 0500 00000";

#[test]
fn golden_qpigs_21_field_parse() {
    let r = parse_status(GOLDEN_QPIGS).expect("golden QPIGS payload parses");

    assert_eq!(r.grid_voltage_dv, 2300);
    assert_eq!(r.grid_frequency_dhz, 500);
    assert_eq!(r.ac_output_voltage_dv, 2300);
    assert_eq!(r.ac_output_frequency_dhz, 500);
    assert_eq!(r.ac_output_apparent_power_va, 1000);
    assert_eq!(r.ac_output_active_power_w, 59);
    assert_eq!(r.bus_voltage_v, 380);
    assert_eq!(r.battery_voltage_cv, 5120);
    assert_eq!(r.battery_charging_current_a, 10);
    assert_eq!(r.inverter_temperature_c, 35);
    assert_eq!(r.pv_current_da, 50);
    assert_eq!(r.pv_voltage_dv, 1850);
    assert_eq!(r.battery_discharge_current_a, 0);
    assert_eq!(r.pv_charging_power_w, 500);
    assert_eq!(r.status_bits, 0x96);
    assert!(r.load_on());
    assert!(r.charging());
    assert!(r.solar_charging());
    assert!(!r.ac_charging());
    // Not carried by QPIGS; left at the "not yet known" placeholders for the caller to merge in.
    assert_eq!(r.mode, InverterMode::Unknown);
    assert_eq!(r.ratings, None);
}

#[test]
fn qpigs_rejects_truncated_payload() {
    // Only 10 of the 21 fields present.
    let short = "230.0 50.0 230.0 50.0 1000 59 00 380 51.20 010";
    assert!(parse_status(short).is_err());
}

#[test]
fn qpigs_tolerates_missing_trailing_field_21() {
    // Field 21 is documented as tolerated/ignored; drop it and confirm the other 20 still parse.
    let missing_21 = "230.0 50.0 230.0 50.0 1000 59 00 380 51.20 010 000 035 005.0 185.0 00000 000 10010110 000 00.0 0500";
    let r = parse_status(missing_21).expect("field 21 is optional");
    assert_eq!(r.pv_charging_power_w, 500);
}

// ---- QMOD: mode single-char decode ----

#[test]
fn qmod_decodes_every_known_letter() {
    assert_eq!(parse_mode("P").unwrap(), InverterMode::PowerOn);
    assert_eq!(parse_mode("S").unwrap(), InverterMode::Standby);
    assert_eq!(parse_mode("L").unwrap(), InverterMode::Line);
    assert_eq!(parse_mode("B").unwrap(), InverterMode::Battery);
    assert_eq!(parse_mode("F").unwrap(), InverterMode::Fault);
    assert_eq!(parse_mode("H").unwrap(), InverterMode::PowerSaving);
}

#[test]
fn qmod_unknown_letter_is_unknown_not_an_error() {
    assert_eq!(parse_mode("Z").unwrap(), InverterMode::Unknown);
}

#[test]
fn qmod_empty_payload_is_unknown_not_an_error() {
    // `"".split(' ')` yields one empty field (not zero fields), so this hits the same "else ->
    // Unknown" catch-all as any other unrecognized character — never a hard parse error, since a
    // mode we can't recognize is still valid information, consistent with `qmod_unknown_letter_*`.
    assert_eq!(parse_mode("").unwrap(), InverterMode::Unknown);
}

// ---- QPIWS: fault-bit permutations at every enabled index ----

/// A `len`-char all-zero binary string with `set` positions flipped to `'1'`.
fn bits_with(set: &[usize], len: usize) -> String {
    (0..len)
        .map(|i| if set.contains(&i) { '1' } else { '0' })
        .collect()
}

#[test]
fn qpiws_all_clear_has_no_flags_and_zero_raw_bits() {
    let f = parse_fault(&bits_with(&[], 20)).expect("all-clear QPIWS decodes");
    assert!(!f.line_fail);
    assert!(!f.overtemperature_fault);
    assert!(!f.fan_locked_fault);
    assert!(!f.battery_low_fault);
    assert!(!f.overload_fault);
    assert!(!f.output_short_fault);
    assert_eq!(f.raw_bits, 0);
}

#[test]
fn qpiws_line_fail_bit_5() {
    let f = parse_fault(&bits_with(&[5], 20)).expect("decodes");
    assert!(f.line_fail);
    assert!(!f.overtemperature_fault);
    assert!(!f.fan_locked_fault);
    assert!(!f.battery_low_fault);
    assert!(!f.overload_fault);
    assert!(!f.output_short_fault);
    assert_eq!(f.raw_bits, 1u64 << 5);
}

#[test]
fn qpiws_overtemperature_bit_9() {
    let f = parse_fault(&bits_with(&[9], 20)).expect("decodes");
    assert!(f.overtemperature_fault);
    assert!(!f.line_fail);
    assert!(!f.fan_locked_fault);
    assert_eq!(f.raw_bits, 1u64 << 9);
}

#[test]
fn qpiws_fan_locked_bit_10() {
    let f = parse_fault(&bits_with(&[10], 20)).expect("decodes");
    assert!(f.fan_locked_fault);
    assert!(!f.overtemperature_fault);
    assert!(!f.battery_low_fault);
    assert_eq!(f.raw_bits, 1u64 << 10);
}

#[test]
fn qpiws_battery_low_bit_12() {
    let f = parse_fault(&bits_with(&[12], 20)).expect("decodes");
    assert!(f.battery_low_fault);
    assert!(!f.fan_locked_fault);
    assert!(!f.overload_fault);
    assert_eq!(f.raw_bits, 1u64 << 12);
}

#[test]
fn qpiws_overload_bit_15() {
    let f = parse_fault(&bits_with(&[15], 20)).expect("decodes");
    assert!(f.overload_fault);
    assert!(!f.battery_low_fault);
    assert!(!f.output_short_fault);
    assert_eq!(f.raw_bits, 1u64 << 15);
}

#[test]
fn qpiws_output_short_bit_17() {
    let f = parse_fault(&bits_with(&[17], 20)).expect("decodes");
    assert!(f.output_short_fault);
    assert!(!f.overload_fault);
    assert_eq!(f.raw_bits, 1u64 << 17);
}

#[test]
fn qpiws_all_enabled_bits_simultaneously() {
    let idx = [5, 9, 10, 12, 15, 17];
    let f = parse_fault(&bits_with(&idx, 20)).expect("decodes");
    assert!(f.line_fail);
    assert!(f.overtemperature_fault);
    assert!(f.fan_locked_fault);
    assert!(f.battery_low_fault);
    assert!(f.overload_fault);
    assert!(f.output_short_fault);
    let expected: u64 = idx.iter().map(|&i| 1u64 << i).sum();
    assert_eq!(f.raw_bits, expected);
}

#[test]
fn qpiws_too_short_is_malformed() {
    // Missing index 17 (output_short_fault) entirely.
    assert!(parse_fault(&bits_with(&[5], 10)).is_err());
}

// ---- QPIRI: field 7 (variant), field 13 (battery_type), field 22 (output_mode) ----

fn qpiri_payload(field7: &str, field13: &str, field22: &str) -> String {
    let mut fields = vec!["0"; 22];
    fields[6] = field7; // 1-based field 7 -> index 6
    fields[12] = field13; // field 13 -> index 12
    fields[21] = field22; // field 22 -> index 21
    fields.join(" ")
}

#[test]
fn qpiri_variant_stores_raw_field_7() {
    let r = parse_ratings(&qpiri_payload("3000", "0", "00")).expect("decodes");
    assert_eq!(r.variant.as_str(), "3000");
}

#[test]
fn qpiri_battery_type_mapping() {
    assert_eq!(
        parse_ratings(&qpiri_payload("x", "0", "00"))
            .unwrap()
            .battery_type
            .as_str(),
        "agm"
    );
    assert_eq!(
        parse_ratings(&qpiri_payload("x", "1", "00"))
            .unwrap()
            .battery_type
            .as_str(),
        "flooded"
    );
    assert_eq!(
        parse_ratings(&qpiri_payload("x", "2", "00"))
            .unwrap()
            .battery_type
            .as_str(),
        "user_defined"
    );
    assert_eq!(
        parse_ratings(&qpiri_payload("x", "3", "00"))
            .unwrap()
            .battery_type
            .as_str(),
        "lithium"
    );
    assert_eq!(
        parse_ratings(&qpiri_payload("x", "9", "00"))
            .unwrap()
            .battery_type
            .as_str(),
        "unknown"
    );
}

#[test]
fn qpiri_output_mode_mapping() {
    let cases = [
        ("00", "single_phase"),
        ("000", "single_phase"),
        ("01", "parallel_single_phase"),
        ("02", "three_phase_p1"),
        ("10", "three_phase_p2"),
        ("20", "three_phase_p3"),
        ("99", "unknown"),
    ];
    for (raw, expected) in cases {
        let r = parse_ratings(&qpiri_payload("x", "0", raw)).expect("decodes");
        assert_eq!(r.output_mode.as_str(), expected, "raw={raw}");
    }
}

#[test]
fn qpiri_short_payload_is_malformed() {
    assert!(parse_ratings("0 0 0").is_err());
}

// ---- QID: whole payload is the serial number ----

#[test]
fn qid_whole_payload_is_serial() {
    let mut out = heapless::String::<32>::new();
    parse_serial("92932002100294", &mut out).expect("decodes");
    assert_eq!(out.as_str(), "92932002100294");
}

#[test]
fn qid_trims_surrounding_whitespace() {
    let mut out = heapless::String::<32>::new();
    parse_serial("  92932002100294  ", &mut out).expect("decodes");
    assert_eq!(out.as_str(), "92932002100294");
}

#[test]
fn qid_overlong_serial_is_capacity_exceeded() {
    let mut out = heapless::String::<32>::new();
    let too_long = "1".repeat(64);
    assert!(parse_serial(&too_long, &mut out).is_err());
}
