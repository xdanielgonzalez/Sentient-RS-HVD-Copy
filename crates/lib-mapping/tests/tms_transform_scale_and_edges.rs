//! HOST unit-test backing for the e2e Robot suite's Seplos->TMS assertions
//! (`distro/qemu/e2e_telemetry.robot`), which today are only verified end-to-end via a slow QEMU
//! boot. This file pins the exact `transform::scale` conversions, the `sample_14_cell()` dataset's
//! derived TMS values, diagnostics, and the bounds/overflow invariants — fast `cargo test`
//! equivalents of the e2e's 6 assertions plus edge cases the e2e can't cheaply exercise.
//!
//! `sample_14_cell()` itself lives in `test-kit` (`crates/test-kit/src/seplos/impl_battery_state.rs`,
//! lines 6-33), which this crate cannot depend on (`test-kit` depends the other way, on
//! `lib-mapping`/`lib-canonical`). Its raw fields are reproduced here field-for-field (matching
//! `tests/wire_value_correctness.rs`'s existing `sample_14_cell()` helper) with citations, so the
//! numbers are traceable to that one canonical source, not invented.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use heapless::Vec as HVec;
use lib_mapping::schema::{ClockMonotonic, Identity};
use lib_mapping::transform::{scale, ToDiagnostics, ToTms};
use lib_parse::canonical::{CanonicalFault, CanonicalReading, MAX_CELLS, MAX_TEMPS};

const TOL: f32 = 0.01;

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < TOL
}

fn dev() -> Identity {
    let mut s = Identity::new();
    s.push_str("METEOR-001").unwrap();
    s
}

fn now() -> ClockMonotonic {
    ClockMonotonic {
        seconds: 1000,
        nanoseconds: 0,
    }
}

/// The e2e's fixture, `test_kit::seplos::BatteryState::sample_14_cell()`
/// (`crates/test-kit/src/seplos/impl_battery_state.rs:6-33`): 14 cells (3800..3813 mV), 6 temps
/// (raw Kelvin×10, 2981..2991 in steps of 2 -> 25.0..26.0 °C), `current_ca=-164` (discharging,
/// vendor convention), `total_voltage_cv=5452`, `soc_dpct=775`, `cycles=2`.
fn sample_14_cell() -> CanonicalReading {
    let mut cells: HVec<u16, MAX_CELLS> = HVec::new();
    for i in 0..14u16 {
        cells.push(3800 + i).unwrap();
    }
    let mut temps: HVec<u16, MAX_TEMPS> = HVec::new();
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

// ---------------------------------------------------------------------------------------------
// 1. DC measurement: mirrors e2e "DC Measurement Carries The Known Seplos Pack Voltage Current
//    And Power" (distro/qemu/e2e_telemetry.robot:46-67).
// ---------------------------------------------------------------------------------------------

#[test]
fn dc_measurement_from_sample_14_cell_matches_e2e_golden_values() {
    let r = sample_14_cell();
    let m = r.to_dc_measurement(dev(), now());

    assert_eq!(m.external_measurement.len(), 1);
    let port = &m.external_measurement[0];
    assert_eq!(port.port_number, 0);
    assert_eq!(port.line.len(), 1);
    let line = &port.line[0];

    // cv_to_v(5452) = 54.52 V (transform.rs:120, scale::cv_to_v).
    assert!(
        approx(line.voltage, 54.52),
        "voltage {} != 54.52",
        line.voltage
    );
    // SIGN INVERSION: vendor charge-positive current_ca=-164 (discharging) -> TMS source-positive
    // -ca_to_a(-164) = +1.64 A (transform.rs:121).
    assert!(
        approx(line.amperage, 1.64),
        "amperage {} != 1.64 (sign inversion)",
        line.amperage
    );
    // power = voltage * amperage = 54.52 * 1.64 = 89.4128 W (transform.rs:125).
    assert!(
        approx(line.power, 89.4128),
        "power {} != 89.4128",
        line.power
    );
    assert!(m.internal_measurement.is_empty());
}

/// **Port-vs-line power path**: `DcPowerPortMeasurement` (`crates/lib-tms/src/generated.rs`,
/// generated from the IDL) has exactly THREE fields — `port_number`, `line: Vec<
/// DcPowerLineMeasurement, MAX_DC_LINES>`, and `power_rate_of_change`. There is NO independent
/// `power` field on the port itself; the only `power` value anywhere in the DC measurement payload
/// lives inside `DcPowerLineMeasurement` (`crates/lib-tms/src/generated.rs`),
/// produced once by `power_line_measurement_from` (transform.rs:119-127) and reused unchanged by
/// `power_port_measurement_from` (transform.rs:129-135) — `bounded([power_line_measurement_from(r)])`
/// wraps the SAME value, it does not recompute a separate port-level power.
///
/// Conclusion: there is no "port power != line power" discrepancy possible in the current schema —
/// asserting `port.line[0].power` IS asserting the only power value that exists. If the e2e's DC
/// test is failing despite matching line values, the cause is NOT a port-vs-line mapping bug in
/// `transform.rs`; it lies elsewhere (JSON field path used by the Robot keyword, wire-vs-struct
/// serialization divergence, broker/topic wiring, or a stale fixture on the QEMU image side).
#[test]
fn power_port_measurement_has_no_independent_power_field_it_reuses_the_line_value() {
    let r = sample_14_cell();
    let port = r.to_dc_measurement(dev(), now()).external_measurement[0].clone();
    let line = port.line[0];

    // The port's only power-bearing data is exactly the line's power — computed once, not twice.
    assert!(approx(line.power, 89.4128), "line power {}", line.power);
    // power_rate_of_change is always NaN (no rate-of-change data from Seplos) — the ONLY other
    // numeric field `PowerPortMeasurement` carries besides `line`.
    assert!(port.power_rate_of_change.is_nan());
}

#[test]
fn dc_measurement_charging_current_inverts_to_negative_amperage() {
    let mut r = sample_14_cell();
    r.current_ca = 500; // vendor charging 5.00 A
    let m = r.to_dc_measurement(dev(), now());
    let line = &m.external_measurement[0].line[0];
    assert!(approx(line.amperage, -5.00), "amperage {}", line.amperage);
    assert!(approx(line.power, 54.52 * -5.00), "power {}", line.power);
}

#[test]
fn dc_measurement_zero_current_yields_zero_power() {
    let mut r = sample_14_cell();
    r.current_ca = 0;
    let m = r.to_dc_measurement(dev(), now());
    let line = &m.external_measurement[0].line[0];
    assert!(approx(line.amperage, 0.0));
    assert!(approx(line.power, 0.0));
}

// ---------------------------------------------------------------------------------------------
// 2. Scale fns directly — exact values + round trips.
// ---------------------------------------------------------------------------------------------

#[test]
fn cv_to_v_exact() {
    assert_eq!(scale::cv_to_v(5452), 54.52);
    assert_eq!(scale::cv_to_v(0), 0.0);
    assert_eq!(scale::cv_to_v(1), 0.01);
}

#[test]
fn cv_to_v_round_trips_through_v_to_cv() {
    for cv in [0u16, 1, 100, 5452, 65535] {
        assert_eq!(scale::v_to_cv(scale::cv_to_v(cv)), cv);
    }
}

#[test]
fn ca_to_a_positive_and_negative() {
    // Positive (vendor charging).
    assert_eq!(scale::ca_to_a(164), 1.64);
    // Negative (vendor discharging) — sample_14_cell's raw value.
    assert_eq!(scale::ca_to_a(-164), -1.64);
    assert_eq!(scale::ca_to_a(0), 0.0);
    assert_eq!(scale::ca_to_a(i16::MIN), i16::MIN as f32 / 100.0);
    assert_eq!(scale::ca_to_a(i16::MAX), i16::MAX as f32 / 100.0);
}

#[test]
fn ca_to_a_round_trips_through_a_to_ca() {
    for ca in [-164i16, -1, 0, 1, 164, 500, i16::MIN + 1, i16::MAX] {
        assert_eq!(scale::a_to_ca(scale::ca_to_a(ca)), ca);
    }
}

#[test]
fn mv_to_v_exact() {
    assert_eq!(scale::mv_to_v(3800), 3.800);
    assert_eq!(scale::mv_to_v(3813), 3.813);
    assert_eq!(scale::mv_to_v(0), 0.0);
}

#[test]
fn mv_to_v_round_trips_through_v_to_mv() {
    for mv in [0u16, 1, 3800, 3813, 65535] {
        assert_eq!(scale::v_to_mv(scale::mv_to_v(mv)), mv);
    }
}

#[test]
fn dpct_to_pu_exact() {
    // sample_14_cell's soc_dpct=775 -> 0.775 p.u.
    assert_eq!(scale::dpct_to_pu(775), 0.775);
    assert_eq!(scale::dpct_to_pu(0), 0.0);
    assert_eq!(scale::dpct_to_pu(1000), 1.0);
    // sub-1% edge (exercised at the wire layer too, wire_value_correctness.rs's
    // negative_amperage_and_sub_one_soc_format_correctly).
    assert_eq!(scale::dpct_to_pu(5), 0.005);
}

#[test]
fn dpct_to_pu_round_trips_through_pu_to_dpct() {
    for dpct in [0u16, 5, 775, 1000] {
        assert_eq!(scale::pu_to_dpct(scale::dpct_to_pu(dpct)), dpct);
    }
}

#[test]
fn dk_to_c_exact_and_boundaries() {
    // 2731 dK -> 0.0 degC (the raw-Kelvin-x10 zero point).
    assert_eq!(scale::dk_to_c(2731), 0.0);
    // 2981 dK -> 25.0 degC (sample_14_cell's sensor 0: 2731 + 250 = 2981).
    assert_eq!(scale::dk_to_c(2981), 25.0);
    // sample_14_cell's sensor 5: 2731 + 250 + 5*2 = 2991 -> 26.0 degC.
    assert_eq!(scale::dk_to_c(2991), 26.0);
    // Below the zero point (valid: raw Kelvin can be < 2731, i.e. sub-zero Celsius).
    assert_eq!(scale::dk_to_c(2681), -5.0);
    assert_eq!(scale::dk_to_c(0), -273.1);
}

#[test]
fn dk_to_c_round_trips_through_c_to_dk() {
    for raw in [0u16, 2681, 2731, 2981, 2991, 65535] {
        assert_eq!(scale::c_to_dk(scale::dk_to_c(raw)), raw);
    }
}

// ---------------------------------------------------------------------------------------------
// 3. Power hardware / SoC / cells: mirrors e2e "Power Hardware Update Carries The Known Storage
//    Cell And SoC Values" (distro/qemu/e2e_telemetry.robot:69-100).
// ---------------------------------------------------------------------------------------------

#[test]
fn power_hardware_from_sample_14_cell_matches_e2e_golden_values() {
    let r = sample_14_cell();
    let p = r.to_power_hardware(dev(), now());
    let es = p.energy_storage.expect("energy_storage present");

    assert_eq!(es.storage_cells.len(), 14, "14 storage cells");

    let cell0 = &es.storage_cells[0];
    assert_eq!(cell0.cell_number, 0);
    // mv_to_v(3800) = 3.800 V.
    assert!(
        approx(cell0.voltage, 3.800),
        "cell0 voltage {}",
        cell0.voltage
    );
    // dpct_to_pu(775) = 0.775 for EVERY cell (Seplos reports one pack-level SOC broadcast
    // identically to every cell — transform.rs FromTms doc comment, lines 401-402).
    // #278: `state_of_charge` is `Option<f32>` (IDL `@optional`); the mapper always sets `Some(_)`.
    assert!(
        approx(cell0.state_of_charge.unwrap(), 0.775),
        "cell0 soc {:?}",
        cell0.state_of_charge
    );
    // nearest_temp assigns sensor 0 (2981 dK = 25.0 degC) to cell 0.
    assert!(
        approx(cell0.temperature, 25.0),
        "cell0 temp {}",
        cell0.temperature
    );
    assert!(approx(cell0.cycle_counter, 2.0));

    let cell13 = &es.storage_cells[13];
    assert_eq!(cell13.cell_number, 13);
    // mv_to_v(3800+13) = 3.813 V.
    assert!(
        approx(cell13.voltage, 3.813),
        "cell13 voltage {}",
        cell13.voltage
    );
    // Every cell shares the same pack-level SoC and cycle count.
    assert!(approx(cell13.state_of_charge.unwrap(), 0.775));
    assert!(approx(cell13.cycle_counter, 2.0));

    // temp_stats over the 6 sensors (25.0..26.0 in 0.2 steps): min=25.0, max=26.0, avg=25.5.
    assert!(approx(es.min_temperature, 25.0));
    assert!(approx(es.max_temperature, 26.0));
    assert!(approx(es.average_temperature, 25.5));
    // Always NaN — no hour-meter data from Seplos (transform.rs:160).
    assert!(es.run_time.is_nan());
}

#[test]
fn power_hardware_cell_voltage_and_soc_are_independent_per_cell_but_soc_is_broadcast() {
    // Distinct per-cell voltages (3800+i mV) map to distinct V values, proving cell_number/voltage
    // track the source index rather than being a constant/broadcast like SoC is.
    let r = sample_14_cell();
    let es = r.to_power_hardware(dev(), now()).energy_storage.unwrap();
    let voltages: std::vec::Vec<f32> = es.storage_cells.iter().map(|c| c.voltage).collect();
    for (i, v) in voltages.iter().enumerate() {
        assert!(
            approx(*v, 3.800 + i as f32 / 1000.0),
            "cell {i} voltage {v}"
        );
    }
    // All 14 SoC values identical (broadcast), not tracking index.
    assert!(es
        .storage_cells
        .iter()
        .all(|c| approx(c.state_of_charge.unwrap(), 0.775)));
}

// ---------------------------------------------------------------------------------------------
// 4. Diagnostics: mirrors e2e "First Cycle Diagnostics Published With Zero Overflow And No
//    Faults" (distro/qemu/e2e_telemetry.robot:102-117).
// ---------------------------------------------------------------------------------------------

#[test]
fn no_faults_reading_publishes_empty_codes_and_zero_overflow() {
    // sample_14_cell sets `faults: FaultFlags::default()` in test-kit (all-clear); the
    // lib-mapping-local equivalent is CanonicalFault::default().
    let f = CanonicalFault::default();
    let ds = f.to_active_diagnostics(dev(), now());
    assert!(ds.codes.is_empty());
    assert_eq!(ds.overflow, 0);
}

#[test]
fn single_fault_flag_produces_exactly_one_dtc_with_correct_shape() {
    let f = CanonicalFault {
        discharge_overcurrent_protection: true,
        ..Default::default()
    };
    let ds = f.to_active_diagnostics(dev(), now());
    assert_eq!(ds.codes.len(), 1);
    assert_eq!(ds.overflow, 0);
    let dtc = &ds.codes[0];
    assert_eq!(dtc.suspect_parameter, 6); // FAULT_TABLE SPN for discharge_overcurrent_protection.
    assert_eq!(
        dtc.failure_mode,
        lib_mapping::schema::FailureModeIndicator::ConditionExists
    );
    assert_eq!(dtc.severity, lib_mapping::schema::DtcSeverity::Major);
    assert_eq!(dtc.occurrence_count, 1);
    assert!(dtc.port_number.is_none());
}

#[test]
fn all_sixteen_fault_flags_produce_sixteen_dtcs_no_overflow() {
    // FAULT_TABLE has exactly 16 entries (transform.rs:207-304); MAX_DTC is 64
    // (crates/lib-tms/src/lib.rs), so a fully-faulted reading can never overflow.
    let f = CanonicalFault {
        device_address: 0,
        high_voltage_protection: true,
        low_voltage_protection: true,
        high_temperature_charge_protection: true,
        low_temperature_charge_protection: true,
        charge_overcurrent_protection: true,
        discharge_overcurrent_protection: true,
        short_circuit_protection: true,
        mosfet_fault: true,
        cell_balance_warning: true,
        low_capacity_warning: true,
        high_temperature_discharge_warning: true,
        low_temperature_discharge_warning: true,
        low_voltage_warning: true,
        high_voltage_warning: true,
        discharge_current_warning: true,
        charge_current_warning: true,
    };
    let ds = f.to_active_diagnostics(dev(), now());
    assert_eq!(ds.codes.len(), 16);
    assert_eq!(ds.overflow, 0);
}

// ---------------------------------------------------------------------------------------------
// 5. Temperatures — boundary values beyond the sample fixture.
// ---------------------------------------------------------------------------------------------

#[test]
fn dk_to_c_reference_boundaries_from_task_spec() {
    // 2731 dK -> 0.0 degC, 2981 dK -> 25.0 degC (explicit boundary pins).
    assert_eq!(scale::dk_to_c(2731), 0.0);
    assert_eq!(scale::dk_to_c(2981), 25.0);
}

#[test]
fn empty_temperatures_yield_nan_triple_not_a_default_zero() {
    let mut r = sample_14_cell();
    r.temperatures_raw = HVec::new();
    let es = r.to_power_hardware(dev(), now()).energy_storage.unwrap();
    assert!(es.min_temperature.is_nan());
    assert!(es.max_temperature.is_nan());
    assert!(es.average_temperature.is_nan());
    // Per-cell nearest_temp is also NaN when there are no sensors at all.
    assert!(es.storage_cells[0].temperature.is_nan());
}

#[test]
fn fewer_sensors_than_cells_clamps_to_last_sensor() {
    // sample_14_cell has 14 cells but only 6 temp sensors; nearest_temp clamps index >= 6 to
    // sensor 5 (2991 dK = 26.0 degC) rather than panicking/OOB.
    let r = sample_14_cell();
    let es = r.to_power_hardware(dev(), now()).energy_storage.unwrap();
    assert!(approx(es.storage_cells[13].temperature, 26.0));
    assert!(approx(es.storage_cells[6].temperature, 26.0));
}

// ---------------------------------------------------------------------------------------------
// 7. Edge/bounds — saturation, sign extremes, overflow guards.
// ---------------------------------------------------------------------------------------------

#[test]
fn cell_voltages_at_max_cells_capacity_all_map_without_truncation() {
    // MAX_CELLS = 16 (crates/lib-canonical/src/lib.rs); MAX_STORAGE_CELLS = 50
    // (crates/lib-tms/src/lib.rs) >> MAX_CELLS, so `bounded()` in energy_storage_state_from
    // (transform.rs:161) can never truncate a valid CanonicalReading's cells — the destination cap
    // always exceeds the source cap. This test documents/pins that invariant at MAX_CELLS.
    let mut r = sample_14_cell();
    let mut cells: HVec<u16, MAX_CELLS> = HVec::new();
    for i in 0..MAX_CELLS as u16 {
        cells.push(3000 + i).unwrap();
    }
    r.cell_voltages_mv = cells;
    let es = r.to_power_hardware(dev(), now()).energy_storage.unwrap();
    assert_eq!(es.storage_cells.len(), MAX_CELLS, "no truncation at cap");
}

#[test]
fn temperatures_at_max_temps_capacity_all_contribute_to_stats() {
    let mut r = sample_14_cell();
    let mut temps: HVec<u16, MAX_TEMPS> = HVec::new();
    for i in 0..MAX_TEMPS as u16 {
        temps.push(2731 + i * 10).unwrap(); // 0, 1, 2, ... degC
    }
    r.temperatures_raw = temps;
    let es = r.to_power_hardware(dev(), now()).energy_storage.unwrap();
    assert!(approx(es.min_temperature, 0.0));
    assert!(approx(es.max_temperature, (MAX_TEMPS - 1) as f32));
}

#[test]
fn extreme_negative_current_saturates_without_panicking() {
    // i16::MIN has no positive counterpart (-i16::MIN overflows i16), so ca_to_a/the sign
    // inversion path must not panic even at this extreme.
    let mut r = sample_14_cell();
    r.current_ca = i16::MIN;
    let m = r.to_dc_measurement(dev(), now());
    let line = &m.external_measurement[0].line[0];
    // -ca_to_a(i16::MIN) = -(-327.68) = 327.68 (computed in f32, no overflow since promoted before
    // negation).
    assert!(approx(line.amperage, 327.68), "amperage {}", line.amperage);
}

#[test]
fn extreme_positive_current_at_i16_max() {
    let mut r = sample_14_cell();
    r.current_ca = i16::MAX;
    let m = r.to_dc_measurement(dev(), now());
    let line = &m.external_measurement[0].line[0];
    assert!(approx(line.amperage, -327.67), "amperage {}", line.amperage);
}

#[test]
fn zero_voltage_and_zero_cells_produce_zero_power_and_empty_cell_list() {
    let r = CanonicalReading {
        device_address: 0,
        cell_voltages_mv: HVec::new(),
        temperatures_raw: HVec::new(),
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
    let dc = r.to_dc_measurement(dev(), now());
    let line = &dc.external_measurement[0].line[0];
    assert_eq!(line.voltage, 0.0);
    assert_eq!(line.amperage, 0.0);
    assert_eq!(line.power, 0.0);

    let hw = r.to_power_hardware(dev(), now());
    assert!(hw.energy_storage.unwrap().storage_cells.is_empty());
}
