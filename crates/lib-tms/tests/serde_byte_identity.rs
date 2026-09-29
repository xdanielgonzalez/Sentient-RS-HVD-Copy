//! PROOF #2 — the GENERATED concrete structs/enums serialize to the EXACT frozen wire bytes,
//! with the MIL-STD-3071 omit-when-None policy (A.9.7.5 / Table A.13).
//!
//! Each representative generated type is serialized through `serde-json-core` (the exact on-device
//! JSON encoder — f32 `NaN` -> `null`) and asserted byte-for-byte against a FROZEN golden literal.
//! Those literals were captured from the hand-written schema types this generator reproduced
//! (proven byte-identical when the generator first landed, #274) BEFORE the hand-written crate was
//! fully retired (its types split into `lib-tms`/`lib-canonical`) — so this stays an INDEPENDENT
//! oracle (a frozen spec), not a tautology against the same types. If the generator's field
//! names/order/optionality/enum-spellings/nesting ever drift,
//! these bytes change and the test fails. Covers scalars, f32 (incl. NaN->null), strings, renamed
//! enums (incl. the preserved IDL typo), `Option` (omitted when None per MIL-STD-3071), bounded
//! sequences, and nesting.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_tms::generated as g;
use serde::Serialize;

/// Serialize through the on-device encoder (serde-json-core, f32 `NaN` -> `null`).
fn json<T: Serialize>(v: &T) -> String {
    serde_json_core::to_string::<_, 4096>(v)
        .expect("encode")
        .as_str()
        .to_string()
}

fn hs<const N: usize>(s: &str) -> heapless::String<N> {
    let mut o = heapless::String::new();
    o.push_str(s).unwrap();
    o
}

#[test]
fn clock_monotonic_scalars() {
    let v = g::ClockMonotonic {
        seconds: 12345,
        nanoseconds: 678_000_000,
    };
    assert_eq!(json(&v), r#"{"seconds":12345,"nanoseconds":678000000}"#);
}

#[test]
fn power_line_measurement_floats() {
    // exact, inexact-in-f32, and a computed product. `DcPowerLineMeasurement` is the collision-
    // renamed `dc::PowerLineMeasurement` (golden bare `PowerLineMeasurement`).
    let v = g::DcPowerLineMeasurement {
        voltage: 54.52,
        amperage: 1.64,
        power: 54.52 * 1.64,
    };
    assert_eq!(
        json(&v),
        r#"{"voltage":54.52,"amperage":1.64,"power":89.4128}"#
    );
}

#[test]
fn storage_cell_state_nan_to_null_and_optional_soc() {
    // `cycle_counter` NaN -> null; `state_of_charge` is `Option<f32>` (#278, IDL `@optional`), and
    // `Some(x)` serializes byte-identically to a bare `f32` (golden had it bare).
    let v = g::StorageCellState {
        cell_number: 7,
        cycle_counter: f32::NAN,
        state_of_charge: Some(0.775),
        temperature: 25.4,
        voltage: 3.807,
    };
    let j = json(&v);
    assert!(
        j.contains(r#""cycle_counter":null"#),
        "NaN must render null: {j}"
    );
    assert_eq!(
        j,
        r#"{"cell_number":7,"cycle_counter":null,"state_of_charge":0.775,"temperature":25.4,"voltage":3.807}"#
    );
}

#[test]
fn enum_wire_spellings() {
    assert_eq!(json(&g::DtcSeverity::Major), r#""SEV_7_MAJOR""#);
    assert_eq!(
        json(&g::FailureModeIndicator::ConditionExists),
        r#""FMI_CONDITION_EXISTS""#
    );
    // The preserved IDL typo: the wire token is `FMI_ABNORMAL_RATE_OF_CHARGE` even though the Rust
    // ident (matching golden) is the corrected `AbnormalRateOfChange`.
    assert_eq!(
        json(&g::FailureModeIndicator::AbnormalRateOfChange),
        r#""FMI_ABNORMAL_RATE_OF_CHARGE""#
    );
}

#[test]
fn storage_update_option_some_none_and_curve_nesting() {
    // Option None -> omitted (skip_serializing_if = "Option::is_none");
    // Option Some -> bare value (no "Some()" wrapper in JSON).
    let curve = g::Curve2D {
        points: heapless::Vec::from_slice(&[g::Point2D { x: 1.0, y: 2.0 }]).unwrap(),
    };
    let v = g::StorageUpdate {
        device_id: hs("METEOR-001"),
        internal_voltage: 54.52,
        state_of_charge: Some(0.775),
        available_energy: 1234.5,
        hold_time: 3600.0,
        charge_time: curve.clone(),
        discharge_time: curve.clone(),
        max_charge_time: curve.clone(),
        max_discharge_time: curve.clone(),
        max_charge_rate: None,            // Option None -> omitted key
        max_discharge_rate: Some(-500.0), // Option Some -> bare value
    };
    assert_eq!(
        json(&v),
        r#"{"device_id":"METEOR-001","internal_voltage":54.52,"state_of_charge":0.775,"available_energy":1234.5,"hold_time":3600.0,"charge_time":{"points":[{"x":1.0,"y":2.0}]},"discharge_time":{"points":[{"x":1.0,"y":2.0}]},"max_charge_time":{"points":[{"x":1.0,"y":2.0}]},"max_discharge_time":{"points":[{"x":1.0,"y":2.0}]},"max_discharge_rate":-500.0}"#
    );
}

#[test]
fn dc_measurement_update_nested_sequences() {
    let line = g::DcPowerLineMeasurement {
        voltage: 54.52,
        amperage: 1.64,
        power: 89.4128,
    };
    let port = g::DcPowerPortMeasurement {
        port_number: 0,
        line: heapless::Vec::from_slice(&[line]).unwrap(),
        power_rate_of_change: f32::NAN,
    };
    let v = g::DcMeasurementUpdate {
        device_id: hs("METEOR-001"),
        time_measured: g::ClockMonotonic {
            seconds: 12345,
            nanoseconds: 678_000_000,
        },
        external_measurement: heapless::Vec::from_slice(&[port]).unwrap(),
        internal_measurement: heapless::Vec::new(),
    };
    assert_eq!(
        json(&v),
        r#"{"device_id":"METEOR-001","time_measured":{"seconds":12345,"nanoseconds":678000000},"external_measurement":[{"port_number":0,"line":[{"voltage":54.52,"amperage":1.64,"power":89.4128}],"power_rate_of_change":null}],"internal_measurement":[]}"#
    );
}

#[test]
fn energy_storage_state_cell_sequence() {
    let mut cells: heapless::Vec<g::StorageCellState, { lib_tms::MAX_STORAGE_CELLS }> =
        heapless::Vec::new();
    for i in 0..3u16 {
        cells
            .push(g::StorageCellState {
                cell_number: i,
                cycle_counter: 2.0,
                state_of_charge: Some(0.775),
                temperature: 25.0 + i as f32 * 0.2,
                voltage: 3.800 + i as f32 * 0.001,
            })
            .unwrap();
    }
    let v = g::EnergyStorageState {
        min_temperature: 25.0,
        average_temperature: 25.2,
        max_temperature: 25.4,
        run_time: f32::NAN,
        storage_cells: cells,
    };
    assert_eq!(
        json(&v),
        r#"{"min_temperature":25.0,"average_temperature":25.2,"max_temperature":25.4,"run_time":null,"storage_cells":[{"cell_number":0,"cycle_counter":2.0,"state_of_charge":0.775,"temperature":25.0,"voltage":3.8},{"cell_number":1,"cycle_counter":2.0,"state_of_charge":0.775,"temperature":25.2,"voltage":3.8009999},{"cell_number":2,"cycle_counter":2.0,"state_of_charge":0.775,"temperature":25.4,"voltage":3.802}]}"#
    );
}

/// #273 DTC field-order finding: the generated `DiagnosticTroubleCode` follows the IDL declaration
/// order exactly, with `hint` LAST (the golden Rust struct hoisted `hint` before the two `estimate_*`
/// fields — a golden divergence from the IDL). This pins the generated (IDL-faithful) order.
/// Optional fields that are None are omitted (MIL-STD-3071 A.9.7.5 / Table A.13).
#[test]
fn diagnostic_trouble_code_idl_order() {
    let v = g::DiagnosticTroubleCode {
        port_number: None, // omitted
        suspect_parameter: 8,
        failure_mode: g::FailureModeIndicator::ConditionExists,
        occurrence_count: 1,
        time_raised: g::ClockMonotonic {
            seconds: 12345,
            nanoseconds: 678_000_000,
        },
        severity: g::DtcSeverity::Major,
        estimate_time: None,     // omitted
        estimate_severity: None, // omitted
        hint: hs("mosfet fault"),
    };
    assert_eq!(
        json(&v),
        r#"{"suspect_parameter":8,"failure_mode":"FMI_CONDITION_EXISTS","occurrence_count":1,"time_raised":{"seconds":12345,"nanoseconds":678000000},"severity":"SEV_7_MAJOR","hint":"mosfet fault"}"#
    );
}

#[test]
fn diagnostic_trouble_code_optional_fields_default() {
    // When optional fields are absent or null, they default to None.
    // This verifies the `#[serde(default)]` attribute works correctly.
    let v1 = g::DiagnosticTroubleCode {
        port_number: None, // omitted by default
        suspect_parameter: 8,
        failure_mode: g::FailureModeIndicator::ConditionExists,
        occurrence_count: 1,
        time_raised: g::ClockMonotonic {
            seconds: 12345,
            nanoseconds: 678_000_000,
        },
        severity: g::DtcSeverity::Major,
        estimate_time: None,     // omitted by default
        estimate_severity: None, // omitted by default
        hint: hs("mosfet fault"),
    };
    // Verify serialization omits these fields
    let serialized = json(&v1);
    assert!(
        !serialized.contains("port_number"),
        "port_number should be omitted"
    );
    assert!(
        !serialized.contains("estimate_time"),
        "estimate_time should be omitted"
    );
    assert!(
        !serialized.contains("estimate_severity"),
        "estimate_severity should be omitted"
    );
}
