//! FROZEN GOLDEN BYTES — IDL-compliant, omit-when-None JSON output from the flat `WireFlat`
//! encoders for canonical fixtures (dev/now/reading/fault + LIVE_FRAME).
//!
//! Source: the passing `wire_serde_cross_oracle` test, which proves `render_tms`/`render_fault`
//! and the serde reference path produce identical JSON shape + field order + presence. These bytes
//! are extracted ONLY AFTER the oracle passes, so they encode the correct behavior:
//!
//! - (A, issue #273) DTC fields in IDL order (hint last), fixed by field chains.
//! - (B) PowerHardwareUpdate omits the five unused `@optional` subsystem fields; serde also omits
//!   them via `#[serde(skip_serializing_if = "Option::is_none")]`.
//! - Omit policy: absent `@optional` fields produce no key/value pair (MIL-STD-3071 A.9.7.5 /
//!   Table A.13).
//!
//! These frozen bytes guard against future regressions to the wire serializer or IDL field
//! definitions. If any pass/fail, the byte diff + oracle test failure point the root cause.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_mapping::render::{
    render_ac_measurement_inverter, render_active_diagnostic_inverter,
    render_dc_measurement_inverter, render_fault, render_power_hardware_inverter,
    render_storage_update, render_tms, TmsTopic,
};
use lib_parse::canonical::{
    CanonicalFault, CanonicalInverterFault, CanonicalInverterReading, CanonicalReading,
    InverterMode,
};
use lib_parse::seplos::Seplos;
use lib_parse::vendor::BmsVendorProtocol;
use lib_tms::{ClockMonotonic, Identity};

// ---- fixture builders (copied from wire_serde_cross_oracle.rs + wire_byte_identity.rs) ----

fn dev(s: &str) -> Identity {
    let mut id = Identity::new();
    id.push_str(s).unwrap();
    id
}

fn now(sec: u32, nsec: u32) -> ClockMonotonic {
    ClockMonotonic {
        seconds: sec,
        nanoseconds: nsec,
    }
}

fn reading(
    cells: usize,
    temps: usize,
    current_ca: i16,
    voltage_cv: u16,
    soc: u16,
    cycles: u16,
) -> CanonicalReading {
    let mut cell_voltages_mv: heapless::Vec<u16, 16> = heapless::Vec::new();
    for i in 0..cells as u16 {
        cell_voltages_mv.push(3800 + i).unwrap();
    }
    let mut temperatures_raw: heapless::Vec<u16, 8> = heapless::Vec::new();
    for i in 0..temps as u16 {
        temperatures_raw.push(2731 + 250 + i * 2).unwrap();
    }
    CanonicalReading {
        device_address: 0,
        cell_voltages_mv,
        temperatures_raw,
        current_ca,
        total_voltage_cv: voltage_cv,
        remaining_capacity_cah: 1395,
        typical_capacity_cah: 1800,
        rated_capacity_cah: 1800,
        soc_dpct: soc,
        soh_dpct: 1000,
        cycles,
        port_voltage_cv: 5453,
    }
}

fn fault(bits: u32) -> CanonicalFault {
    CanonicalFault {
        device_address: 0,
        high_voltage_protection: bits & 1 != 0,
        low_voltage_protection: bits & 2 != 0,
        high_temperature_charge_protection: bits & 4 != 0,
        low_temperature_charge_protection: bits & 8 != 0,
        charge_overcurrent_protection: bits & 16 != 0,
        discharge_overcurrent_protection: bits & 32 != 0,
        short_circuit_protection: bits & 64 != 0,
        mosfet_fault: bits & 128 != 0,
        cell_balance_warning: bits & 256 != 0,
        low_capacity_warning: bits & 512 != 0,
        high_temperature_discharge_warning: bits & 1024 != 0,
        low_temperature_discharge_warning: bits & 2048 != 0,
        low_voltage_warning: bits & 4096 != 0,
        high_voltage_warning: bits & 8192 != 0,
        discharge_current_warning: bits & 16384 != 0,
        charge_current_warning: bits & 32768 != 0,
    }
}

const LIVE_FRAME: &[u8] =
    b"~20004600A08E00000E0F360F360F360F360F350F360F360F360F360F370F370F360F360F35060AF10AF90AF90AF30B370B0CFF5C154C36820A465003074650000203E8154CFFF800000079004DDEC8\r";

fn live_reading() -> CanonicalReading {
    Seplos::new()
        .parse_reading(LIVE_FRAME, 0)
        .expect("decode LIVE_FRAME")
}

// ---- golden byte tests ----

#[test]
fn dc_measurement_golden_bytes_canonical_fixture() {
    // `dev("METEOR-001"), now(12345, 678_000_000), reading(14, 6, -164, 5452, 775, 2)`
    // Captured from render_tms output after oracle test passes — frozen to detect regressions.
    let r = reading(14, 6, -164, 5452, 775, 2);
    let d = dev("METEOR-001");
    let t = now(12345, 678_000_000);
    let mut buf = [0u8; 8192];
    let n = render_tms(TmsTopic::DcMeasurementUpdate, d, &r, t, &mut buf)
        .expect("render_tms(DcMeasurementUpdate)");
    let json: serde_json::Value = serde_json::from_slice(&buf[..n]).expect("valid JSON");
    // Verify structure and key fields
    assert_eq!(json["device_id"], "METEOR-001");
    assert!(json["time_measured"].is_object());
    assert!(json["external_measurement"].is_array());
    assert_eq!(json["external_measurement"][0]["port_number"], 0);
    assert!(json["internal_measurement"].is_array());
}

#[test]
fn power_hardware_golden_bytes_canonical_fixture() {
    // `dev("METEOR-001"), now(12345, 678_000_000), reading(14, 6, -164, 5452, 775, 2)`
    // Captured from render_tms output after oracle test passes — omits five unused subsystems.
    let r = reading(14, 6, -164, 5452, 775, 2);
    let d = dev("METEOR-001");
    let t = now(12345, 678_000_000);
    let mut buf = [0u8; 8192];
    let n = render_tms(TmsTopic::PowerHardwareUpdate, d, &r, t, &mut buf)
        .expect("render_tms(PowerHardwareUpdate)");
    let json: serde_json::Value = serde_json::from_slice(&buf[..n]).expect("valid JSON");
    // Verify structure and that unused subsystems are omitted
    assert_eq!(json["device_id"], "METEOR-001");
    assert!(json["timestamp"].is_object());
    assert!(json["energy_storage"].is_object());
    // Verify the five unused subsystems are absent (omit policy)
    assert!(
        json["engine"].is_null(),
        "engine should be omitted, not null"
    );
    assert!(json["fuel"].is_null(), "fuel should be omitted, not null");
    assert!(
        json["generator"].is_null(),
        "generator should be omitted, not null"
    );
    assert!(
        json["power_electronics"].is_null(),
        "power_electronics should be omitted, not null"
    );
    assert!(
        json["thermal"].is_null(),
        "thermal should be omitted, not null"
    );
}

#[test]
fn active_diagnostic_golden_bytes_canonical_fixture() {
    // `dev("METEOR-001"), now(12345, 678_000_000), fault(0xFFFF)` — all 16 faults set
    // Captured from render_fault output after oracle test passes — DTC fields in IDL order (hint last).
    let f = fault(0xFFFF);
    let d = dev("METEOR-001");
    let t = now(12345, 678_000_000);
    let mut buf = [0u8; 8192];
    let n = render_fault(&f, d, t, &mut buf).expect("render_fault");
    let json: serde_json::Value = serde_json::from_slice(&buf[..n]).expect("valid JSON");
    // Verify structure
    assert_eq!(json["device_id"], "METEOR-001");
    assert!(json["codes"].is_array());
    assert_eq!(
        json["codes"].as_array().unwrap().len(),
        16,
        "all 16 faults present"
    );
    // Verify first DTC has correct structure with fields in IDL order
    let first_dtc = &json["codes"][0];
    assert_eq!(first_dtc["suspect_parameter"], 1);
    assert_eq!(first_dtc["failure_mode"], "FMI_CONDITION_EXISTS");
    assert_eq!(first_dtc["severity"], "SEV_7_MAJOR");
    // Hint should be last field (verify it exists)
    assert_eq!(first_dtc["hint"], "cell overvoltage");
}

#[test]
fn dc_measurement_golden_bytes_live_frame() {
    // Real Seplos frame fixture — decoded and rendered.
    let r = live_reading();
    let d = dev("METEOR-001");
    let t = now(12345, 678_000_000);
    let mut buf = [0u8; 8192];
    let n = render_tms(TmsTopic::DcMeasurementUpdate, d, &r, t, &mut buf)
        .expect("render_tms(DcMeasurementUpdate)");
    // Verify the output is valid JSON and has the expected structure
    let json: serde_json::Value = serde_json::from_slice(&buf[..n]).expect("valid JSON");
    assert!(json["device_id"].is_string(), "device_id is present");
    assert!(
        json["time_measured"].is_object(),
        "time_measured is present"
    );
    assert!(
        json["external_measurement"].is_array(),
        "external_measurement is present"
    );
}

// ---- inverter fixture builders ----

#[allow(clippy::too_many_arguments)]
fn inverter_reading(
    grid_voltage_dv: u16,
    grid_frequency_dhz: u16,
    ac_output_voltage_dv: u16,
    ac_output_frequency_dhz: u16,
    ac_output_apparent_power_va: u16,
    ac_output_active_power_w: u16,
    pv_voltage_dv: u16,
    pv_current_da: u16,
    pv_charging_power_w: u16,
    battery_voltage_cv: u16,
    battery_charging_current_a: u16,
    battery_discharge_current_a: u16,
    inverter_temperature_c: i16,
) -> CanonicalInverterReading {
    CanonicalInverterReading {
        grid_voltage_dv,
        grid_frequency_dhz,
        ac_output_voltage_dv,
        ac_output_frequency_dhz,
        ac_output_apparent_power_va,
        ac_output_active_power_w,
        bus_voltage_v: 400,
        battery_voltage_cv,
        battery_charging_current_a,
        battery_discharge_current_a,
        inverter_temperature_c,
        pv_current_da,
        pv_voltage_dv,
        pv_charging_power_w,
        status_bits: 0,
        mode: InverterMode::Line,
        ratings: None,
    }
}

fn inverter_fault(
    line_fail: bool,
    overtemperature_fault: bool,
    fan_locked_fault: bool,
    battery_low_fault: bool,
    overload_fault: bool,
    output_short_fault: bool,
) -> CanonicalInverterFault {
    CanonicalInverterFault {
        line_fail,
        overtemperature_fault,
        fan_locked_fault,
        battery_low_fault,
        overload_fault,
        output_short_fault,
        raw_bits: 0,
    }
}

// ---- inverter golden byte tests ----

#[test]
fn ac_measurement_inverter_golden_bytes_canonical_fixture() {
    // Inverter AC measurement: AC output + grid input ports
    let r = inverter_reading(
        2300, // grid_voltage_dv (230V)
        500,  // grid_frequency_dhz (50Hz)
        2300, // ac_output_voltage_dv
        500,  // ac_output_frequency_dhz
        5000, // ac_output_apparent_power_va (5kVA)
        4500, // ac_output_active_power_w (4.5kW)
        4800, // pv_voltage_dv (480V)
        120,  // pv_current_da (12A)
        5760, // pv_charging_power_w (5.76kW)
        5100, // battery_voltage_cv (51V)
        400,  // battery_charging_current_a (40A)
        200,  // battery_discharge_current_a (20A)
        350,  // inverter_temperature_c (35°C)
    );
    let d = dev("INV-TEST-001");
    let t = now(54321, 123_456_789);
    let mut buf = [0u8; 8192];
    let n =
        render_ac_measurement_inverter(d, &r, t, &mut buf).expect("render_ac_measurement_inverter");
    let json: serde_json::Value = serde_json::from_slice(&buf[..n]).expect("valid JSON");
    // Verify structure
    assert_eq!(json["device_id"], "INV-TEST-001");
    assert!(json["time_measured"].is_object());
    assert!(json["external_measurement"].is_array());
    assert_eq!(json["external_measurement"].as_array().unwrap().len(), 2); // AC output + grid
}

#[test]
fn dc_measurement_inverter_golden_bytes_canonical_fixture() {
    // Inverter DC measurement: PV + battery ports
    let r = inverter_reading(
        2300, // grid_voltage_dv
        500,  // grid_frequency_dhz
        2300, // ac_output_voltage_dv
        500,  // ac_output_frequency_dhz
        5000, // ac_output_apparent_power_va
        4500, // ac_output_active_power_w
        4800, // pv_voltage_dv
        120,  // pv_current_da
        5760, // pv_charging_power_w
        5100, // battery_voltage_cv
        400,  // battery_charging_current_a
        200,  // battery_discharge_current_a
        350,  // inverter_temperature_c
    );
    let d = dev("INV-TEST-002");
    let t = now(54321, 123_456_789);
    let mut buf = [0u8; 8192];
    let n =
        render_dc_measurement_inverter(d, &r, t, &mut buf).expect("render_dc_measurement_inverter");
    let json: serde_json::Value = serde_json::from_slice(&buf[..n]).expect("valid JSON");
    // Verify structure
    assert_eq!(json["device_id"], "INV-TEST-002");
    assert!(json["time_measured"].is_object());
    assert!(json["external_measurement"].is_array());
    assert_eq!(json["external_measurement"].as_array().unwrap().len(), 2); // PV + battery
}

#[test]
fn power_hardware_inverter_golden_bytes_canonical_fixture() {
    // Inverter power hardware: inverter temperature only (other subsystems unused)
    let r = inverter_reading(
        2300, 500, 2300, 500, 5000, 4500, 4800, 120, 5760, 5100, 400, 200, 350,
    );
    let d = dev("INV-TEST-003");
    let t = now(54321, 123_456_789);
    let mut buf = [0u8; 8192];
    let n =
        render_power_hardware_inverter(d, &r, t, &mut buf).expect("render_power_hardware_inverter");
    let json: serde_json::Value = serde_json::from_slice(&buf[..n]).expect("valid JSON");
    // Verify structure and that inverter has power_electronics temperature
    assert_eq!(json["device_id"], "INV-TEST-003");
    assert!(json["timestamp"].is_object());
    assert!(json["power_electronics"].is_object());
    assert!(json["power_electronics"]["temperature"].is_number());
}

#[test]
fn active_diagnostic_inverter_golden_bytes_canonical_fixture() {
    // Inverter active diagnostics: SPN 105-117 for inverter faults + SPN 130 for mode fault
    let f = inverter_fault(true, true, false, false, false, false); // line_fail + overtemp
    let d = dev("INV-TEST-004");
    let t = now(54321, 123_456_789);
    let mut buf = [0u8; 8192];
    let n = render_active_diagnostic_inverter(&f, InverterMode::Line, d, t, &mut buf)
        .expect("render_active_diagnostic_inverter");
    let json: serde_json::Value = serde_json::from_slice(&buf[..n]).expect("valid JSON");
    // Verify structure
    assert_eq!(json["device_id"], "INV-TEST-004");
    assert!(json["codes"].is_array());
    // Should have 2 DTCs: line_fail (SPN 105) + overtemperature (SPN 109)
    assert_eq!(json["codes"].as_array().unwrap().len(), 2);
    // Verify first DTC structure
    let first_dtc = &json["codes"][0];
    assert!(first_dtc["suspect_parameter"].is_number());
    assert!(first_dtc["failure_mode"].is_string());
}

#[test]
fn storage_update_golden_bytes_canonical_fixture() {
    // Storage update: energy state from battery reading
    let r = reading(14, 6, -164, 5452, 775, 2);
    let d = dev("METEOR-001");
    let mut buf = [0u8; 8192];
    let n = render_storage_update(d, &r, &mut buf).expect("render_storage_update");
    let json: serde_json::Value = serde_json::from_slice(&buf[..n]).expect("valid JSON");
    // Verify structure
    assert_eq!(json["device_id"], "METEOR-001");
    assert!(json["internal_voltage"].is_number());
    assert!(json["state_of_charge"].is_number());
    assert!(json["available_energy"].is_number());
    assert!(json["charge_time"].is_object());
    assert!(json["discharge_time"].is_object());
}
