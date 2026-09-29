//! Value-correctness for the flat wire serializer (`crate::wire`) — Phase 1 gate. Parses the
//! emitted JSON with `serde_json` (test-only dependency) and asserts the values match the plan's
//! golden fixture: `sample_14_cell` (voltage 54.52, amperage +1.64 sign-inverted, soc 0.775, 14
//! cells, cycles 2.0, temps ~25) + a 4-fault vector. Byte-exactness vs the old `ryu` output is NOT
//! required (fixed-precision differs) — values are asserted within the TMS accuracy tolerance
//! (+/-0.005, `docs/tms-compliance.md`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_mapping::wire::{
    serialize_active_diagnostic, serialize_dc_measurement, serialize_into, serialize_power_hardware,
};
use lib_parse::canonical::{CanonicalFault, CanonicalReading};
use lib_tms::ClockMonotonic;
use serde_json::Value;

const TOL: f64 = 0.005;

fn assert_close(actual: f64, expected: f64, label: &str) {
    assert!(
        (actual - expected).abs() <= TOL,
        "{label}: got {actual}, expected {expected} +/- {TOL}"
    );
}

/// The plan's golden reading: 14 cells (3800..3813 mV), 6 temps (25.0..26.0C in 0.2C steps),
/// current -164 cA (discharging, vendor convention), voltage 5452 cV, soc 775 dpct, cycles 2 —
/// originally mirroring `test_kit::seplos::BatteryState::sample_14_cell()` (this crate has no
/// dependency on `test-kit`, so the fixture is reproduced directly against `CanonicalReading`
/// rather than imported). Note: `sample_14_cell()` has since made its `total_voltage_cv`
/// self-consistent with the cell sum (5329); this standalone fixture deliberately keeps 5452 —
/// the values here only need to be internally stable for the wire-value assertions below.
fn sample_14_cell() -> CanonicalReading {
    let mut cells: heapless::Vec<u16, 16> = heapless::Vec::new();
    for i in 0..14u16 {
        cells.push(3800 + i).unwrap();
    }
    let mut temps: heapless::Vec<u16, 8> = heapless::Vec::new();
    for i in 0..6u16 {
        temps.push(2731 + 250 + i * 2).unwrap();
    }
    CanonicalReading {
        device_address: 0,
        cell_voltages_mv: cells,
        temperatures_raw: temps,
        current_ca: -164,
        total_voltage_cv: 5452,
        remaining_capacity_cah: 1395,
        typical_capacity_cah: 1800,
        rated_capacity_cah: 1800,
        soc_dpct: 775,
        soh_dpct: 1000,
        cycles: 2,
        port_voltage_cv: 5453,
    }
}

fn device_id() -> lib_tms::Identity {
    let mut id = lib_tms::Identity::new();
    id.push_str("METEOR-001").unwrap();
    id
}

fn now() -> ClockMonotonic {
    ClockMonotonic {
        seconds: 12345,
        nanoseconds: 678_000_000,
    }
}

/// 4 faults set (out of 16), matching the plan's "4-fault vector".
fn four_fault_vector() -> CanonicalFault {
    CanonicalFault {
        device_address: 0,
        high_voltage_protection: true,
        low_temperature_charge_protection: true,
        mosfet_fault: true,
        discharge_current_warning: true,
        ..Default::default()
    }
}

#[test]
fn dc_measurement_values_match_goldens() {
    let reading = sample_14_cell();
    let dev = device_id();
    let mut buf = [0u8; 4096];
    let n = serialize_into(&mut buf, |s| {
        serialize_dc_measurement(&reading, &dev, now(), s)
    })
    .expect("fits in 4096 bytes");
    let s = core::str::from_utf8(&buf[..n]).expect("valid utf8");
    let v: Value = serde_json::from_str(s).expect("valid JSON");

    assert_eq!(v["device_id"], "METEOR-001");
    assert_eq!(v["time_measured"]["seconds"], 12345);
    assert_eq!(v["time_measured"]["nanoseconds"], 678_000_000);

    let line = &v["external_measurement"][0]["line"][0];
    assert_close(line["voltage"].as_f64().unwrap(), 54.52, "voltage");
    assert_close(
        line["amperage"].as_f64().unwrap(),
        1.64,
        "amperage (sign-inverted)",
    );
    assert_close(line["power"].as_f64().unwrap(), 54.52 * 1.64, "power");
    assert!(v["external_measurement"][0]["power_rate_of_change"].is_null());
    assert!(v["internal_measurement"].as_array().unwrap().is_empty());
}

#[test]
fn power_hardware_values_match_goldens() {
    let reading = sample_14_cell();
    let dev = device_id();
    let mut buf = [0u8; 4096];
    let n = serialize_into(&mut buf, |s| {
        serialize_power_hardware(&reading, &dev, now(), s)
    })
    .expect("fits in 4096 bytes");
    let s = core::str::from_utf8(&buf[..n]).expect("valid utf8");
    let v: Value = serde_json::from_str(s).expect("valid JSON");

    assert_eq!(v["device_id"], "METEOR-001");
    let es = &v["energy_storage"];
    assert_close(
        es["min_temperature"].as_f64().unwrap(),
        25.0,
        "min_temperature",
    );
    assert_close(
        es["average_temperature"].as_f64().unwrap(),
        25.5,
        "average_temperature",
    );
    assert_close(
        es["max_temperature"].as_f64().unwrap(),
        26.0,
        "max_temperature",
    );
    assert!(es["run_time"].is_null());

    let cells = es["storage_cells"].as_array().unwrap();
    assert_eq!(cells.len(), 14, "14 cells");
    for (i, cell) in cells.iter().enumerate() {
        assert_eq!(cell["cell_number"], i as u64);
        assert_close(
            cell["cycle_counter"].as_f64().unwrap(),
            2.0,
            "cycle_counter",
        );
        assert_close(
            cell["state_of_charge"].as_f64().unwrap(),
            0.775,
            "state_of_charge",
        );
        let expected_voltage = (3800 + i) as f64 / 1000.0;
        assert_close(
            cell["voltage"].as_f64().unwrap(),
            expected_voltage,
            "cell voltage",
        );
        assert!(
            cell["temperature"].as_f64().unwrap() >= 24.9
                && cell["temperature"].as_f64().unwrap() <= 26.1
        );
    }
}

#[test]
fn active_diagnostic_values_match_goldens() {
    let fault = four_fault_vector();
    let dev = device_id();
    let mut buf = [0u8; 4096];
    let n = serialize_into(&mut buf, |s| {
        serialize_active_diagnostic(&fault, &dev, now(), s)
    })
    .expect("fits in 4096 bytes");
    let s = core::str::from_utf8(&buf[..n]).expect("valid utf8");
    let v: Value = serde_json::from_str(s).expect("valid JSON");

    assert_eq!(v["device_id"], "METEOR-001");
    let codes = v["codes"].as_array().unwrap();
    assert_eq!(codes.len(), 4, "4 DTCs");
    assert_eq!(v["overflow"], 0);

    let spns: Vec<u64> = codes
        .iter()
        .map(|c| c["suspect_parameter"].as_u64().unwrap())
        .collect();
    assert_eq!(
        spns,
        vec![1, 4, 8, 15],
        "SPN order matches FAULT_TABLE order"
    );

    for code in codes {
        assert!(code["port_number"].is_null());
        assert_eq!(code["failure_mode"], "FMI_CONDITION_EXISTS");
        assert_eq!(code["occurrence_count"], 1);
        assert_eq!(code["time_raised"]["seconds"], 12345);
        assert_eq!(code["severity"], "SEV_7_MAJOR");
        assert!(code["estimate_time"].is_null());
        assert!(code["estimate_severity"].is_null());
        // `affected_identity_id` was removed (not in the 2.0.2 IDL) — assert it is absent, not null.
        assert!(code.get("affected_identity_id").is_none());
    }
}

#[test]
fn empty_reading_and_fault_produce_valid_json() {
    let reading = CanonicalReading {
        device_address: 0,
        cell_voltages_mv: heapless::Vec::new(),
        temperatures_raw: heapless::Vec::new(),
        current_ca: 0,
        total_voltage_cv: 0,
        remaining_capacity_cah: 0,
        typical_capacity_cah: 0,
        rated_capacity_cah: 0,
        soc_dpct: 0,
        soh_dpct: 0,
        cycles: 0,
        port_voltage_cv: 0,
    };
    let dev = device_id();
    let mut buf = [0u8; 1024];

    let n = serialize_into(&mut buf, |s| {
        serialize_dc_measurement(&reading, &dev, now(), s)
    })
    .unwrap();
    let v: Value = serde_json::from_str(core::str::from_utf8(&buf[..n]).unwrap()).unwrap();
    assert_close(
        v["external_measurement"][0]["line"][0]["voltage"]
            .as_f64()
            .unwrap(),
        0.0,
        "zero voltage",
    );

    let mut buf2 = [0u8; 1024];
    let n2 = serialize_into(&mut buf2, |s| {
        serialize_power_hardware(&reading, &dev, now(), s)
    })
    .unwrap();
    let v2: Value = serde_json::from_str(core::str::from_utf8(&buf2[..n2]).unwrap()).unwrap();
    assert!(
        v2["energy_storage"]["min_temperature"].is_null(),
        "no sensors -> null triple"
    );
    assert!(v2["energy_storage"]["storage_cells"]
        .as_array()
        .unwrap()
        .is_empty());

    let fault = CanonicalFault::default();
    let mut buf3 = [0u8; 1024];
    let n3 = serialize_into(&mut buf3, |s| {
        serialize_active_diagnostic(&fault, &dev, now(), s)
    })
    .unwrap();
    let v3: Value = serde_json::from_str(core::str::from_utf8(&buf3[..n3]).unwrap()).unwrap();
    assert!(v3["codes"].as_array().unwrap().is_empty());
    assert_eq!(v3["overflow"], 0);
}

#[test]
fn negative_amperage_and_sub_one_soc_format_correctly() {
    // current_ca positive (vendor charging) -> inverted TMS amperage is NEGATIVE (sourcing out of
    // the microgrid convention would be positive; a positive vendor current means charging, so
    // post-inversion amperage is negative) -- exercises the negative-fixed-point path end to end.
    let mut reading = sample_14_cell();
    reading.current_ca = 250; // vendor charging 2.50A
    reading.soc_dpct = 5; // 0.5% -> 0.005 pu, exercises the sub-0.01 leading-zero path.

    let dev = device_id();
    let mut buf = [0u8; 4096];
    let n = serialize_into(&mut buf, |s| {
        serialize_dc_measurement(&reading, &dev, now(), s)
    })
    .unwrap();
    let v: Value = serde_json::from_str(core::str::from_utf8(&buf[..n]).unwrap()).unwrap();
    assert_close(
        v["external_measurement"][0]["line"][0]["amperage"]
            .as_f64()
            .unwrap(),
        -2.50,
        "inverted amperage negative",
    );

    let mut buf2 = [0u8; 4096];
    let n2 = serialize_into(&mut buf2, |s| {
        serialize_power_hardware(&reading, &dev, now(), s)
    })
    .unwrap();
    let v2: Value = serde_json::from_str(core::str::from_utf8(&buf2[..n2]).unwrap()).unwrap();
    assert_close(
        v2["energy_storage"]["storage_cells"][0]["state_of_charge"]
            .as_f64()
            .unwrap(),
        0.005,
        "sub-0.01 soc",
    );
}
