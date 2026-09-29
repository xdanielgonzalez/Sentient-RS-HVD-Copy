//! Canonical → TMS transform acceptance vectors.
//!
//! Checks the load-bearing conventions from MIL-STD-3071 §2.7 and the Seplos→TMS mapping:
//! sign inversion (vendor charge-positive → TMS microgrid-positive), unit scaling (cV/cA/mV →
//! V/A, raw Kelvin×10 → °C, deci-percent → per-unit), and fault → DiagnosticTroubleCode.

// Test code may unwrap/panic; the workspace denies these in logic only.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use heapless::{String, Vec};
use lib_mapping::schema::{ClockMonotonic, DtcSeverity, FailureModeIndicator};
use lib_mapping::transform::{FromDiagnostics, FromTms, ToDiagnostics, ToTms};
use lib_parse::canonical::{CanonicalFault, CanonicalReading};

fn dev() -> String<32> {
    let mut s = String::new();
    s.push_str("METEOR-001").ok();
    s
}

fn now() -> ClockMonotonic {
    ClockMonotonic {
        seconds: 1000,
        nanoseconds: 0,
    }
}

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

/// A reading with known raw values. Discharging at 1.44 A, pack 54.51 V, 3 cells, 2 temps @ 7.0/8.0 °C.
fn sample_reading() -> CanonicalReading {
    let mut cells = Vec::new();
    cells.push(3894).ok(); // -> 3.894 V
    cells.push(3893).ok();
    cells.push(3895).ok();
    let mut temps = Vec::new();
    temps.push(2801).ok(); // (2801-2731)/10 = 7.0 °C
    temps.push(2811).ok(); // 8.0 °C
    CanonicalReading {
        device_address: 0,
        cell_voltages_mv: cells,
        temperatures_raw: temps,
        current_ca: -144,       // discharging 1.44 A
        total_voltage_cv: 5451, // 54.51 V
        remaining_capacity_cah: 13953,
        typical_capacity_cah: 18000,
        rated_capacity_cah: 18000,
        soc_dpct: 775, // 77.5% -> 0.775 p.u.
        soh_dpct: 1000,
        cycles: 2,
        port_voltage_cv: 5453,
    }
}

#[test]
fn dc_measurement_sign_and_units() {
    let r = sample_reading();
    let m = r.to_dc_measurement(dev(), now());
    assert_eq!(m.external_measurement.len(), 1);
    let port = &m.external_measurement[0];
    assert_eq!(port.port_number, 0);
    assert_eq!(port.line.len(), 1);
    let line = &port.line[0];
    assert!(approx(line.voltage, 54.51));
    // Discharging (vendor -1.44 A) -> TMS positive (into microgrid) +1.44 A.
    assert!(approx(line.amperage, 1.44));
    assert!(approx(line.power, 54.51 * 1.44));
    assert!(m.internal_measurement.is_empty());
}

#[test]
fn dc_measurement_charging_inverts_negative() {
    let mut r = sample_reading();
    r.current_ca = 500; // charging 5.00 A
    let m = r.to_dc_measurement(dev(), now());
    let line = &m.external_measurement[0].line[0];
    // Charging (vendor +5.00 A) -> TMS negative (drawn from microgrid) -5.00 A.
    assert!(approx(line.amperage, -5.00));
}

#[test]
fn power_hardware_energy_storage() {
    let r = sample_reading();
    let p = r.to_power_hardware(dev(), now());
    let es = p.energy_storage.expect("energy_storage present");
    assert!(approx(es.min_temperature, 7.0));
    assert!(approx(es.max_temperature, 8.0));
    assert!(approx(es.average_temperature, 7.5));
    assert_eq!(es.storage_cells.len(), 3);
    assert!(approx(es.storage_cells[0].voltage, 3.894)); // 3894 mV -> V
    assert_eq!(es.storage_cells[0].cell_number, 0);
    // #278: `state_of_charge` is now `Option<f32>` (IDL `@optional`); the mapper always sets it.
    assert!(approx(es.storage_cells[0].state_of_charge.unwrap(), 0.775));
    assert!(approx(es.storage_cells[0].cycle_counter, 2.0));
}

#[test]
fn fault_maps_to_diagnostic_codes() {
    let mut f = CanonicalFault {
        device_address: 0,
        ..Default::default()
    };
    f.high_voltage_protection = true;
    f.short_circuit_protection = true;
    let ds = f.to_active_diagnostics(dev(), now());
    assert_eq!(ds.codes.len(), 2);
    assert_eq!(ds.overflow, 0);
    for c in ds.codes.iter() {
        assert_eq!(c.failure_mode, FailureModeIndicator::ConditionExists);
        assert_eq!(c.severity, DtcSeverity::Major);
        assert_eq!(c.occurrence_count, 1);
        assert!(c.port_number.is_none());
        assert!(!c.hint.is_empty());
        // Seplos carries no prognostic model — these two @optional IDL fields are always absent on
        // write.
        assert!(c.estimate_time.is_none());
        assert!(c.estimate_severity.is_none());
    }
}

#[test]
fn diagnostics_round_trip_recovers_known_flags_and_drops_unknown_spn() {
    let mut f = CanonicalFault {
        device_address: 0,
        ..Default::default()
    };
    f.high_voltage_protection = true;
    f.short_circuit_protection = true;
    let ds = f.to_active_diagnostics(dev(), now());

    let decoded = CanonicalFault::from_active_diagnostics(&ds);
    assert_eq!(decoded, f);

    // A DTC whose suspect_parameter is not one of the Seplos SPNs has no canonical flag to set —
    // it must be dropped, not misattributed to some other flag.
    let mut foreign = ds.clone();
    foreign.codes[0].suspect_parameter = 999_999;
    let decoded_foreign = CanonicalFault::from_active_diagnostics(&foreign);
    assert!(!decoded_foreign.high_voltage_protection);
    assert!(decoded_foreign.short_circuit_protection);
}

#[test]
fn from_tms_recovers_voltage_current_soc_and_cycles() {
    let r = sample_reading();
    let dc = r.to_dc_measurement(dev(), now());
    let hw = r.to_power_hardware(dev(), now());

    let decoded = CanonicalReading::from_tms(&dc, &hw);
    assert_eq!(decoded.total_voltage_cv, r.total_voltage_cv);
    assert_eq!(decoded.current_ca, r.current_ca);
    assert_eq!(decoded.cell_voltages_mv, r.cell_voltages_mv);
    assert_eq!(decoded.soc_dpct, r.soc_dpct);
    assert_eq!(decoded.cycles, r.cycles);
    // Not representable in TMS at all (see `FromTms` doc) — explicit zero placeholder, not a
    // claim that the pack actually has zero capacity.
    assert_eq!(decoded.remaining_capacity_cah, 0);
    assert_eq!(decoded.typical_capacity_cah, 0);
    assert_eq!(decoded.rated_capacity_cah, 0);
    assert_eq!(decoded.soh_dpct, 0);
    assert_eq!(decoded.port_voltage_cv, 0);
}

#[test]
fn from_tms_reconstructs_temps_exactly_when_cells_outnumber_sensors() {
    // sample_reading() has 3 cells and 2 temperature sensors (7.0, 8.0 degC) — more cells than
    // sensors, the case `FromTms` documents as an exact reconstruction.
    let r = sample_reading();
    let hw = r.to_power_hardware(dev(), now());
    let dc = r.to_dc_measurement(dev(), now());
    let decoded = CanonicalReading::from_tms(&dc, &hw);
    assert_eq!(
        decoded.temperatures_raw.as_slice(),
        r.temperatures_raw.as_slice()
    );
}

#[test]
fn from_tms_charging_current_reinverts_sign_correctly() {
    let mut r = sample_reading();
    r.current_ca = 500; // charging 5.00 A (vendor convention: positive = charging)
    let dc = r.to_dc_measurement(dev(), now());
    let hw = r.to_power_hardware(dev(), now());
    let decoded = CanonicalReading::from_tms(&dc, &hw);
    assert_eq!(decoded.current_ca, 500);
}

#[test]
fn no_faults_no_codes() {
    let f = CanonicalFault {
        device_address: 0,
        ..Default::default()
    };
    let ds = f.to_active_diagnostics(dev(), now());
    assert!(ds.codes.is_empty());
    assert_eq!(ds.overflow, 0);
}
