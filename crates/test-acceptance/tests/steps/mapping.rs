//! Step definitions for the cucumber spec-alignment suite.
//!
//! **No Seplos protocol logic and no Canonical->TMS transform logic is defined in this file.**
//! Every decode/map/codec operation below is a direct call into the real, reused crates:
//! `test_kit::seplos` (the shared-checksum `SeplosSim`), `lib_parse::seplos`
//! (the real Seplos decoder), `lib_mapping::transform` (the real Canonical->TMS mapper +
//! scale helpers), and `lib_mapping::codec` (the real JSON/binary wire codecs). What lives
//! here is Gherkin<->Rust plumbing only: building request frames, matching a fault-flag name to
//! its struct field, and asserting on the results — never re-deriving how a frame is built,
//! checksummed, scaled, or mapped.
//!
//! Test code only; freely uses unwrap/expect/panic (the workspace lint gate denies these in
//! production logic, not tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cucumber::{given, then, when};

use heapless::Vec as HVec;
use lib_mapping::codec::to_json_slice;
use lib_mapping::schema::{
    ActiveDiagnosticState, DcMeasurementUpdate, DtcSeverity, FailureModeIndicator,
    PowerHardwareUpdate, StorageCellState,
};
use lib_mapping::transform::{scale, FromDiagnostics, FromTms, ToDiagnostics, ToTms};
use lib_parse::canonical::{CanonicalFault, CanonicalReading, MAX_CELLS, MAX_TEMPS};
use lib_parse::seplos::command::{build_command, Command};
use lib_parse::seplos::Seplos;
use lib_parse::vendor::BmsVendorProtocol;
use test_kit::seplos::{BatteryState, Behavior, FaultFlags, SeplosSim};

use crate::{approx, dev, now, SpecWorld};

/// The real 14-cell telemetry response (Seplos 48150-1101-10E, DIP 0) — copied verbatim from
/// `lib-mapping/tests/tms_live_frame.rs` (itself cross-checked against
/// `lib-parse/tests/seplos_vectors.rs::decodes_live_telemetry`), so this suite binds to the
/// exact same known-good bytes as the Rust unit test.
const LIVE_FRAME: &[u8] = b"~20004600A08E00000E0F360F360F360F360F350F360F360F360F360F370F370F360F360F35060AF10AF90AF90AF30B370B0CFF5C154C36820A465003074650000203E8154CFFF800000079004DDEC8\r";

/// A `CanonicalReading` fixture identical to `tms_transform.rs`'s private `sample_reading()`
/// (3 cells, 2 temperature sensors, discharging 1.44 A) — literal test data, copied verbatim so
/// this suite's structural scenarios exercise the same known values as the Rust acceptance
/// vectors, not a fresh set of assumptions.
fn sample_reading() -> CanonicalReading {
    let mut cells: HVec<u16, MAX_CELLS> = HVec::new();
    cells.push(3894).expect("push cell"); // -> 3.894 V
    cells.push(3893).expect("push cell");
    cells.push(3895).expect("push cell");
    let mut temps: HVec<u16, MAX_TEMPS> = HVec::new();
    temps.push(2801).expect("push temp"); // (2801-2731)/10 = 7.0 degC
    temps.push(2811).expect("push temp"); // 8.0 degC
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

/// A `CanonicalReading` with every field zeroed/empty except `current_ca` — used by the sign
/// convention Scenario Outline, where only the current -> amperage mapping is under test.
fn reading_with_current(current_ca: i16) -> CanonicalReading {
    CanonicalReading {
        device_address: 0,
        cell_voltages_mv: HVec::new(),
        temperatures_raw: HVec::new(),
        current_ca,
        total_voltage_cv: 0,
        remaining_capacity_cah: 0,
        typical_capacity_cah: 0,
        rated_capacity_cah: 0,
        soc_dpct: 0,
        soh_dpct: 0,
        cycles: 0,
        port_voltage_cv: 0,
    }
}

/// Set one of `CanonicalFault`'s 16 boolean flags by its Gherkin `{word}` name. Panics on an
/// unrecognised name (a feature-file typo, not a runtime condition to handle gracefully).
fn set_fault_flag(fault: &mut CanonicalFault, name: &str) {
    let slot = match name {
        "high_voltage_protection" => &mut fault.high_voltage_protection,
        "low_voltage_protection" => &mut fault.low_voltage_protection,
        "high_temperature_charge_protection" => &mut fault.high_temperature_charge_protection,
        "low_temperature_charge_protection" => &mut fault.low_temperature_charge_protection,
        "charge_overcurrent_protection" => &mut fault.charge_overcurrent_protection,
        "discharge_overcurrent_protection" => &mut fault.discharge_overcurrent_protection,
        "short_circuit_protection" => &mut fault.short_circuit_protection,
        "mosfet_fault" => &mut fault.mosfet_fault,
        "cell_balance_warning" => &mut fault.cell_balance_warning,
        "low_capacity_warning" => &mut fault.low_capacity_warning,
        "high_temperature_discharge_warning" => &mut fault.high_temperature_discharge_warning,
        "low_temperature_discharge_warning" => &mut fault.low_temperature_discharge_warning,
        "low_voltage_warning" => &mut fault.low_voltage_warning,
        "high_voltage_warning" => &mut fault.high_voltage_warning,
        "discharge_current_warning" => &mut fault.discharge_current_warning,
        "charge_current_warning" => &mut fault.charge_current_warning,
        other => panic!("unknown fault flag {other:?} in feature file"),
    };
    *slot = true;
}

/// Read one of `CanonicalFault`'s 16 boolean flags by its Gherkin `{word}` name.
fn get_fault_flag(fault: &CanonicalFault, name: &str) -> bool {
    match name {
        "high_voltage_protection" => fault.high_voltage_protection,
        "low_voltage_protection" => fault.low_voltage_protection,
        "high_temperature_charge_protection" => fault.high_temperature_charge_protection,
        "low_temperature_charge_protection" => fault.low_temperature_charge_protection,
        "charge_overcurrent_protection" => fault.charge_overcurrent_protection,
        "discharge_overcurrent_protection" => fault.discharge_overcurrent_protection,
        "short_circuit_protection" => fault.short_circuit_protection,
        "mosfet_fault" => fault.mosfet_fault,
        "cell_balance_warning" => fault.cell_balance_warning,
        "low_capacity_warning" => fault.low_capacity_warning,
        "high_temperature_discharge_warning" => fault.high_temperature_discharge_warning,
        "low_temperature_discharge_warning" => fault.low_temperature_discharge_warning,
        "low_voltage_warning" => fault.low_voltage_warning,
        "high_voltage_warning" => fault.high_voltage_warning,
        "discharge_current_warning" => fault.discharge_current_warning,
        "charge_current_warning" => fault.charge_current_warning,
        other => panic!("unknown fault flag {other:?} in feature file"),
    }
}

/// Set one of the simulator's `FaultFlags` by its Gherkin `{word}` name (same 16 names as
/// `CanonicalFault`, mirrored onto the simulator's own flag struct).
fn set_sim_fault_flag(flags: &mut FaultFlags, name: &str) {
    let slot = match name {
        "high_voltage_protection" => &mut flags.high_voltage_protection,
        "low_voltage_protection" => &mut flags.low_voltage_protection,
        "high_temperature_charge_protection" => &mut flags.high_temperature_charge_protection,
        "low_temperature_charge_protection" => &mut flags.low_temperature_charge_protection,
        "charge_overcurrent_protection" => &mut flags.charge_overcurrent_protection,
        "discharge_overcurrent_protection" => &mut flags.discharge_overcurrent_protection,
        "short_circuit_protection" => &mut flags.short_circuit_protection,
        "mosfet_fault" => &mut flags.mosfet_fault,
        "cell_balance_warning" => &mut flags.cell_balance_warning,
        "low_capacity_warning" => &mut flags.low_capacity_warning,
        "high_temperature_discharge_warning" => &mut flags.high_temperature_discharge_warning,
        "low_temperature_discharge_warning" => &mut flags.low_temperature_discharge_warning,
        "low_voltage_warning" => &mut flags.low_voltage_warning,
        "high_voltage_warning" => &mut flags.high_voltage_warning,
        "discharge_current_warning" => &mut flags.discharge_current_warning,
        "charge_current_warning" => &mut flags.charge_current_warning,
        other => panic!("unknown fault flag {other:?} in feature file"),
    };
    *slot = true;
}

/// A `CanonicalFault` with all 16 protection/warning flags set (the maximum possible from a
/// Seplos device — used by the DTC-overflow-headroom scenario).
fn all_flags_set() -> CanonicalFault {
    CanonicalFault {
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
    }
}

fn storage_cell(world: &SpecWorld, i: usize) -> StorageCellState {
    // #278: `StorageCellState` is no longer `Copy` (it carries an `Option<f32>` now), so clone it out
    // of the slice rather than moving.
    world
        .hw
        .as_ref()
        .expect("hw computed")
        .energy_storage
        .as_ref()
        .expect("energy_storage present")
        .storage_cells[i]
        .clone()
}

fn cell_voltages_mv(world: &SpecWorld) -> Vec<u32> {
    world
        .hw
        .as_ref()
        .expect("hw computed")
        .energy_storage
        .as_ref()
        .expect("energy_storage present")
        .storage_cells
        .iter()
        .map(|c| (c.voltage * 1000.0).round() as u32)
        .collect()
}

fn set_behavior(world: &mut SpecWorld, behavior: Behavior) {
    world
        .sim
        .as_mut()
        .expect("simulator initialized")
        .set_behavior(behavior);
    world.behavior = Some(behavior);
}

// ---- Given: background / fixture setup ------------------------------------------------------

#[given("the 14-cell sample battery")]
fn given_14_cell_battery(world: &mut SpecWorld) {
    let state = BatteryState::sample_14_cell();
    world.sim = Some(SeplosSim::new(state.clone()));
    world.state = Some(state);
}

#[given("the tms_transform sample reading (3 cells, 2 temps, discharging 1.44 A)")]
fn given_sample_reading(world: &mut SpecWorld) {
    world.reading = Some(sample_reading());
}

#[given("the real 14-cell LIVE_FRAME capture from a Seplos 48150-1101-10E (DIP 0)")]
fn given_live_frame(world: &mut SpecWorld) {
    world.raw_frame = LIVE_FRAME.to_vec();
    world.dip = 0;
}

#[given(expr = "a canonical reading with pack current {int} centi-amps")]
fn given_reading_with_current(world: &mut SpecWorld, centi_amps: i64) {
    world.reading = Some(reading_with_current(centi_amps as i16));
}

#[given(expr = "a canonical fault with flag {word} set")]
fn given_fault_flag(world: &mut SpecWorld, name: String) {
    let fault = world.fault.get_or_insert_with(|| CanonicalFault {
        device_address: 0,
        ..Default::default()
    });
    set_fault_flag(fault, &name);
}

#[given("a canonical fault with no flags set")]
fn given_no_faults(world: &mut SpecWorld) {
    world.fault = Some(CanonicalFault {
        device_address: 0,
        ..Default::default()
    });
}

#[given("a canonical fault with every flag set")]
fn given_all_faults(world: &mut SpecWorld) {
    world.fault = Some(all_flags_set());
}

#[given(expr = "the simulated battery reports fault flag {word} set")]
fn given_sim_fault_flag(world: &mut SpecWorld, name: String) {
    let sim = world.sim.as_mut().expect("simulator initialized");
    set_sim_fault_flag(&mut sim.state_mut().faults, &name);
}

#[given("the simulator behavior is corrupted checksum")]
fn given_behavior_corrupt_checksum(world: &mut SpecWorld) {
    set_behavior(world, Behavior::CorruptChecksum);
}

#[given("the simulator behavior is a wrong address reply")]
fn given_behavior_wrong_address(world: &mut SpecWorld) {
    set_behavior(world, Behavior::WrongAddress);
}

#[given(expr = "the simulator behavior is truncated to {int} bytes")]
fn given_behavior_truncated(world: &mut SpecWorld, n: i64) {
    set_behavior(world, Behavior::Truncated(n as usize));
}

#[given("the simulator behavior is silence")]
fn given_behavior_silence(world: &mut SpecWorld) {
    set_behavior(world, Behavior::Silence);
}

#[given("the simulator behavior is garbage bytes")]
fn given_behavior_garbage(world: &mut SpecWorld) {
    set_behavior(world, Behavior::Garbage);
}

// ---- When: actions (all delegate to the real decode/map/codec) ------------------------------

#[when(expr = "device {int} polls for telemetry")]
fn when_polls_telemetry(world: &mut SpecWorld, dip: i64) {
    let dip = dip as u8;
    world.dip = dip;
    let mut req: HVec<u8, 32> = HVec::new();
    build_command(dip, Command::Telemetry, &mut req).expect("build telemetry request");
    let reply = world
        .sim
        .as_mut()
        .expect("simulator initialized")
        .respond(&req);
    world.raw_frame = reply.as_bytes().to_vec();
    match Seplos::new().parse_reading(&world.raw_frame, dip) {
        Ok(r) => {
            world.reading = Some(r);
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(e.to_string());
            world.reading = None;
        }
    }
}

#[when(expr = "device {int} polls for faults")]
fn when_polls_faults(world: &mut SpecWorld, dip: i64) {
    let dip = dip as u8;
    world.dip = dip;
    let mut req: HVec<u8, 32> = HVec::new();
    build_command(dip, Command::Fault, &mut req).expect("build fault request");
    let reply = world
        .sim
        .as_mut()
        .expect("simulator initialized")
        .respond(&req);
    world.raw_frame = reply.as_bytes().to_vec();
    match Seplos::new().parse_fault(&world.raw_frame, dip) {
        Ok(f) => {
            world.fault = Some(f);
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(e.to_string());
            world.fault = None;
        }
    }
}

#[when("the frame is decoded into a canonical reading")]
fn when_decode_frame(world: &mut SpecWorld) {
    let frame = world.raw_frame.clone();
    match Seplos::new().parse_reading(&frame, world.dip) {
        Ok(r) => {
            world.reading = Some(r);
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(e.to_string());
        }
    }
}

#[when("the reading is mapped to a DC measurement")]
fn when_map_dc(world: &mut SpecWorld) {
    let reading = world.reading.clone().expect("reading decoded");
    world.dc = Some(reading.to_dc_measurement(dev(), now()));
}

#[when("the reading is mapped to power hardware")]
fn when_map_hw(world: &mut SpecWorld) {
    let reading = world.reading.clone().expect("reading decoded");
    world.hw = Some(reading.to_power_hardware(dev(), now()));
}

#[when("the faults are mapped to active diagnostics")]
fn when_map_diag(world: &mut SpecWorld) {
    let fault = world.fault.expect("fault set");
    world.diag = Some(fault.to_active_diagnostics(dev(), now()));
}

#[when("the dc and hw payloads round-trip through JSON")]
fn when_round_trip_json(world: &mut SpecWorld) {
    let dc = world.dc.clone().expect("dc computed");
    let hw = world.hw.clone().expect("hw computed");

    let mut dc_buf = [0u8; 4096];
    let dc_n = to_json_slice(&dc, &mut dc_buf).expect("serialize dc");
    let (dc_decoded, _): (DcMeasurementUpdate, usize) =
        serde_json_core::from_slice(&dc_buf[..dc_n]).expect("deserialize dc");

    let mut hw_buf = [0u8; 4096];
    let hw_n = to_json_slice(&hw, &mut hw_buf).expect("serialize hw");
    let (hw_decoded, _): (PowerHardwareUpdate, usize) =
        serde_json_core::from_slice(&hw_buf[..hw_n]).expect("deserialize hw");

    world.round_tripped = Some(CanonicalReading::from_tms(&dc_decoded, &hw_decoded));
    world.dc = Some(dc_decoded);
    world.hw = Some(hw_decoded);
}

#[when("the active diagnostic state round-trips through JSON")]
fn when_diag_round_trip_json(world: &mut SpecWorld) {
    let diag = world.diag.clone().expect("diagnostics computed");

    let mut buf = [0u8; 8192];
    let n = to_json_slice(&diag, &mut buf).expect("serialize diagnostics");
    let (decoded, _): (ActiveDiagnosticState, usize) =
        serde_json_core::from_slice(&buf[..n]).expect("deserialize diagnostics");

    world.diag = Some(decoded.clone());
    world.fault = Some(CanonicalFault::from_active_diagnostics(&decoded));
}

#[when(expr = "diagnostic code {int} suspect parameter is overwritten to {int}")]
fn when_overwrite_suspect_parameter(world: &mut SpecWorld, index: i64, spn: i64) {
    let diag = world.diag.as_mut().expect("diagnostics computed");
    diag.codes[index as usize].suspect_parameter = spn as u32;
}

#[when("the diagnostics are decoded back into canonical fault flags")]
fn when_decode_diagnostics(world: &mut SpecWorld) {
    let diag = world.diag.clone().expect("diagnostics computed");
    world.fault = Some(CanonicalFault::from_active_diagnostics(&diag));
}

// ---- Then: assertions -------------------------------------------------------------------------

#[then("the read should fail")]
fn then_read_fails(world: &mut SpecWorld) {
    assert!(
        world.last_error.is_some(),
        "expected a decode error, got none"
    );
}

#[then("the read should succeed")]
fn then_read_succeeds(world: &mut SpecWorld) {
    assert!(
        world.last_error.is_none(),
        "expected no decode error, got {:?}",
        world.last_error
    );
}

#[then(expr = "there is {int} external measurement port")]
#[then(expr = "there are {int} external measurement ports")]
fn then_external_ports(world: &mut SpecWorld, n: i64) {
    let dc = world.dc.as_ref().expect("dc computed");
    assert_eq!(dc.external_measurement.len(), n as usize);
}

#[then("the internal measurement is empty")]
fn then_internal_empty(world: &mut SpecWorld) {
    let dc = world.dc.as_ref().expect("dc computed");
    assert!(dc.internal_measurement.is_empty());
}

#[then(expr = "the DC line voltage is approximately {float} V")]
fn then_dc_voltage(world: &mut SpecWorld, expected: f64) {
    let dc = world.dc.as_ref().expect("dc computed");
    let actual = dc.external_measurement[0].line[0].voltage;
    assert!(
        approx(actual, expected as f32),
        "voltage {actual} !~ {expected}"
    );
}

#[then(expr = "the DC line amperage is approximately {float} A")]
fn then_dc_amperage(world: &mut SpecWorld, expected: f64) {
    let dc = world.dc.as_ref().expect("dc computed");
    let actual = dc.external_measurement[0].line[0].amperage;
    assert!(
        approx(actual, expected as f32),
        "amperage {actual} !~ {expected}"
    );
}

#[then(expr = "the DC line power is approximately {float} W")]
fn then_dc_power(world: &mut SpecWorld, expected: f64) {
    let dc = world.dc.as_ref().expect("dc computed");
    let actual = dc.external_measurement[0].line[0].power;
    assert!(
        approx(actual, expected as f32),
        "power {actual} !~ {expected}"
    );
}

#[then(expr = "there are {int} storage cells")]
#[then(expr = "there is {int} storage cell")]
fn then_storage_cell_count(world: &mut SpecWorld, n: i64) {
    let hw = world.hw.as_ref().expect("hw computed");
    let es = hw.energy_storage.as_ref().expect("energy_storage present");
    assert_eq!(es.storage_cells.len(), n as usize);
}

#[then(expr = "storage cell {int} voltage is approximately {float} V")]
fn then_cell_voltage(world: &mut SpecWorld, i: i64, expected: f64) {
    let cell = storage_cell(world, i as usize);
    assert!(
        approx(cell.voltage, expected as f32),
        "cell[{i}] voltage {} !~ {expected}",
        cell.voltage
    );
}

#[then(expr = "storage cell {int} state of charge is approximately {float}")]
fn then_cell_soc(world: &mut SpecWorld, i: i64, expected: f64) {
    let cell = storage_cell(world, i as usize);
    // #278: `state_of_charge` is `Option<f32>` (IDL `@optional`); the mapper always sets `Some(_)`.
    assert!(approx(cell.state_of_charge.unwrap(), expected as f32));
}

#[then(expr = "storage cell {int} cycle counter is approximately {float}")]
fn then_cell_cycles(world: &mut SpecWorld, i: i64, expected: f64) {
    let cell = storage_cell(world, i as usize);
    assert!(approx(cell.cycle_counter, expected as f32));
}

#[then(expr = "storage cell {int} temperature is approximately {float} degC")]
fn then_cell_temp(world: &mut SpecWorld, i: i64, expected: f64) {
    let cell = storage_cell(world, i as usize);
    assert!(approx(cell.temperature, expected as f32));
}

#[then(expr = "the minimum cell voltage in millivolts is {int}")]
fn then_min_cell_mv(world: &mut SpecWorld, expected: i64) {
    let mv = cell_voltages_mv(world)
        .into_iter()
        .min()
        .expect("at least one cell");
    assert_eq!(mv, expected as u32);
}

#[then(expr = "the maximum cell voltage in millivolts is {int}")]
fn then_max_cell_mv(world: &mut SpecWorld, expected: i64) {
    let mv = cell_voltages_mv(world)
        .into_iter()
        .max()
        .expect("at least one cell");
    assert_eq!(mv, expected as u32);
}

#[then(expr = "the minimum temperature is approximately {float} degC")]
fn then_min_temp(world: &mut SpecWorld, expected: f64) {
    let hw = world.hw.as_ref().expect("hw computed");
    let es = hw.energy_storage.as_ref().expect("energy_storage present");
    assert!(approx(es.min_temperature, expected as f32));
}

#[then(expr = "the maximum temperature is approximately {float} degC")]
fn then_max_temp(world: &mut SpecWorld, expected: f64) {
    let hw = world.hw.as_ref().expect("hw computed");
    let es = hw.energy_storage.as_ref().expect("energy_storage present");
    assert!(approx(es.max_temperature, expected as f32));
}

#[then(expr = "the average temperature is approximately {float} degC")]
fn then_avg_temp(world: &mut SpecWorld, expected: f64) {
    let hw = world.hw.as_ref().expect("hw computed");
    let es = hw.energy_storage.as_ref().expect("energy_storage present");
    assert!(approx(es.average_temperature, expected as f32));
}

#[then(expr = "there are {int} diagnostic codes")]
#[then(expr = "there is {int} diagnostic code")]
fn then_diag_code_count(world: &mut SpecWorld, n: i64) {
    let diag = world.diag.as_ref().expect("diagnostics computed");
    assert_eq!(diag.codes.len(), n as usize);
}

#[then(expr = "there is {int} diagnostic overflow")]
fn then_diag_overflow(world: &mut SpecWorld, n: i64) {
    let diag = world.diag.as_ref().expect("diagnostics computed");
    assert_eq!(diag.overflow, n as u32);
}

#[then("every diagnostic code is a Major ConditionExists fault with no optional fields")]
fn then_every_code_major(world: &mut SpecWorld) {
    let diag = world.diag.as_ref().expect("diagnostics computed");
    for code in diag.codes.iter() {
        assert_eq!(code.failure_mode, FailureModeIndicator::ConditionExists);
        assert_eq!(code.severity, DtcSeverity::Major);
        assert_eq!(code.occurrence_count, 1);
        assert!(code.port_number.is_none());
        assert!(!code.hint.is_empty());
        assert!(code.estimate_time.is_none());
        assert!(code.estimate_severity.is_none());
    }
}

#[then(expr = "diagnostic code {int} has suspect parameter {int}")]
fn then_code_spn(world: &mut SpecWorld, i: i64, spn: i64) {
    let diag = world.diag.as_ref().expect("diagnostics computed");
    assert_eq!(diag.codes[i as usize].suspect_parameter, spn as u32);
}

#[then(expr = "the decoded fault has flag {word} set")]
fn then_decoded_flag_set(world: &mut SpecWorld, name: String) {
    let fault = world.fault.as_ref().expect("fault decoded");
    assert!(
        get_fault_flag(fault, &name),
        "expected flag {name} to be set"
    );
}

#[then(expr = "the decoded fault has flag {word} not set")]
fn then_decoded_flag_not_set(world: &mut SpecWorld, name: String) {
    let fault = world.fault.as_ref().expect("fault decoded");
    assert!(
        !get_fault_flag(fault, &name),
        "expected flag {name} to be clear"
    );
}

#[then("the round-tripped reading matches the original telemetry")]
fn then_round_trip_matches_original(world: &mut SpecWorld) {
    let original = world.reading.clone().expect("original reading decoded");
    let decoded = world
        .round_tripped
        .clone()
        .expect("round-tripped reading computed");
    assert_eq!(decoded.total_voltage_cv, original.total_voltage_cv);
    assert_eq!(decoded.current_ca, original.current_ca);
    assert_eq!(decoded.cell_voltages_mv, original.cell_voltages_mv);
    assert_eq!(decoded.soc_dpct, original.soc_dpct);
    assert_eq!(decoded.cycles, original.cycles);
}

#[then("the round-tripped temperatures raw match the original")]
fn then_round_trip_temps_match(world: &mut SpecWorld) {
    let original = world.reading.clone().expect("original reading decoded");
    let decoded = world
        .round_tripped
        .clone()
        .expect("round-tripped reading computed");
    assert_eq!(decoded.temperatures_raw, original.temperatures_raw);
}

// ---- Then: transform_conventions.feature's scaling/sign matrix (delegates to `scale::*`) ------

#[then(expr = "{int} centi-volts scales to approximately {float} V via cv_to_v")]
fn then_cv_to_v(_world: &mut SpecWorld, raw: i64, expected: f64) {
    assert!(approx(scale::cv_to_v(raw as u16), expected as f32));
}

#[then(expr = "{int} millivolts scales to approximately {float} V via mv_to_v")]
fn then_mv_to_v(_world: &mut SpecWorld, raw: i64, expected: f64) {
    assert!(approx(scale::mv_to_v(raw as u16), expected as f32));
}

#[then(expr = "{int} raw Kelvin x10 scales to approximately {float} degC via dk_to_c")]
fn then_dk_to_c(_world: &mut SpecWorld, raw: i64, expected: f64) {
    assert!(approx(scale::dk_to_c(raw as u16), expected as f32));
}

#[then(expr = "{int} deci-percent scales to approximately {float} per-unit via dpct_to_pu")]
fn then_dpct_to_pu(_world: &mut SpecWorld, raw: i64, expected: f64) {
    assert!(approx(scale::dpct_to_pu(raw as u16), expected as f32));
}
