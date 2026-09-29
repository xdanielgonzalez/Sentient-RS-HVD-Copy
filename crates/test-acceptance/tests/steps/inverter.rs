//! Step definitions for the RS-232 Voltronic inverter transform feature area (D10, PROVISIONAL).
//!
//! **No Voltronic protocol logic and no Canonical->TMS transform logic is defined here.** Every
//! decode/map below is a direct call into the real, reused crates: `test_kit::VoltronicSim` (the
//! shared-CRC sim), `lib_parse::voltronic` (the real Voltronic decoder), and
//! `lib_mapping::transform_inverter` (the real inverter CONV mapper). What lives here is
//! Gherkin<->Rust plumbing only.
//!
//! Test code only; freely uses unwrap/expect/panic (the workspace lint gate denies these in
//! production logic, not tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cucumber::{given, then, when};

use lib_mapping::schema::{
    AcMeasurementUpdate, ActiveDiagnosticState, DcMeasurementUpdate, DtcSeverity,
    FailureModeIndicator, PowerHardwareUpdate,
};
use lib_mapping::transform_inverter::{
    to_ac_measurement, to_active_diagnostics, to_dc_measurement, to_power_hardware,
};
use lib_parse::canonical::{CanonicalInverterFault, CanonicalInverterReading, InverterMode};
use lib_parse::vendor::InverterVendorProtocol;
use lib_parse::voltronic::Voltronic;
use test_kit::VoltronicSim;

use crate::{approx, dev, now, SpecWorld};

/// Scenario scratch for the inverter transform feature area. `cucumber` resets `SpecWorld` per
/// scenario. All `Option`/defaulted, populated lazily by whichever steps run.
#[derive(Debug, Default)]
pub struct InverterState {
    reading: Option<CanonicalInverterReading>,
    fault: Option<CanonicalInverterFault>,
    mode: Option<InverterMode>,
    ac: Option<AcMeasurementUpdate>,
    dc: Option<DcMeasurementUpdate>,
    hw: Option<PowerHardwareUpdate>,
    diag: Option<ActiveDiagnosticState>,
}

#[given("a Voltronic QPIGS status reading from the sim")]
fn given_qpigs(world: &mut SpecWorld) {
    let sim = VoltronicSim::new();
    let bytes = sim.respond(b"QPIGS\r").expect("QPIGS reply");
    let reading = Voltronic::new()
        .parse_status(&bytes)
        .expect("QPIGS decodes");
    world.inverter.reading = Some(reading);
}

#[given("a Voltronic QPIWS fault with the overload bit set")]
fn given_qpiws_overload(world: &mut SpecWorld) {
    let mut sim = VoltronicSim::new();
    sim.set_overload_fault();
    let bytes = sim.respond(b"QPIWS\r").expect("QPIWS reply");
    let fault = Voltronic::new().parse_fault(&bytes).expect("QPIWS decodes");
    world.inverter.fault = Some(fault);
}

#[given("the inverter is in Line mode")]
fn given_mode_line(world: &mut SpecWorld) {
    world.inverter.mode = Some(InverterMode::Line);
}

#[given("the inverter reports a Fault mode")]
fn given_mode_fault(world: &mut SpecWorld) {
    world.inverter.mode = Some(InverterMode::Fault);
}

#[when("the inverter reading is mapped to an AC measurement")]
fn when_map_ac(world: &mut SpecWorld) {
    let r = world.inverter.reading.as_ref().expect("reading set");
    world.inverter.ac = Some(to_ac_measurement(r, dev(), now()));
}

#[when("the inverter reading is mapped to a DC measurement")]
fn when_map_dc(world: &mut SpecWorld) {
    let r = world.inverter.reading.as_ref().expect("reading set");
    world.inverter.dc = Some(to_dc_measurement(r, dev(), now()));
}

#[when("the inverter reading is mapped to power hardware")]
fn when_map_hw(world: &mut SpecWorld) {
    let r = world.inverter.reading.as_ref().expect("reading set");
    world.inverter.hw = Some(to_power_hardware(r, dev(), now()));
}

#[when("the inverter fault is mapped to diagnostics")]
fn when_map_diag(world: &mut SpecWorld) {
    let f = world.inverter.fault.as_ref().expect("fault set");
    let mode = world.inverter.mode.unwrap_or(InverterMode::Unknown);
    world.inverter.diag = Some(to_active_diagnostics(f, mode, dev(), now()));
}

#[when("the inverter mode is mapped to diagnostics")]
fn when_map_mode_diag(world: &mut SpecWorld) {
    let mode = world.inverter.mode.unwrap_or(InverterMode::Unknown);
    let fault = CanonicalInverterFault::default();
    world.inverter.diag = Some(to_active_diagnostics(&fault, mode, dev(), now()));
}

#[then(expr = "there are {int} external AC measurement ports")]
fn then_ac_port_count(world: &mut SpecWorld, n: usize) {
    let ac = world.inverter.ac.as_ref().expect("ac mapped");
    assert_eq!(ac.external_measurement.len(), n);
}

#[then(expr = "AC port {int} voltage is approximately {float} V")]
fn then_ac_port_voltage(world: &mut SpecWorld, port: i32, volts: f32) {
    let ac = world.inverter.ac.as_ref().expect("ac mapped");
    let p = ac
        .external_measurement
        .iter()
        .find(|p| p.port_number == port)
        .unwrap_or_else(|| panic!("no AC port {port}"));
    assert!(
        approx(p.line[0].voltage, volts),
        "AC port {port} voltage {} !~ {volts}",
        p.line[0].voltage
    );
}

#[then(expr = "AC port {int} amperage is not available")]
fn then_ac_port_amperage_nan(world: &mut SpecWorld, port: i32) {
    let ac = world.inverter.ac.as_ref().expect("ac mapped");
    let p = ac
        .external_measurement
        .iter()
        .find(|p| p.port_number == port)
        .unwrap_or_else(|| panic!("no AC port {port}"));
    assert!(
        p.line[0].amperage.is_nan(),
        "AC port {port} amperage must be NaN (unknown from QPIGS)"
    );
}

#[then(expr = "there are {int} external DC measurement ports")]
fn then_dc_port_count(world: &mut SpecWorld, n: usize) {
    let dc = world.inverter.dc.as_ref().expect("dc mapped");
    assert_eq!(dc.external_measurement.len(), n);
}

#[then(expr = "DC port {int} voltage is approximately {float} V")]
fn then_dc_port_voltage(world: &mut SpecWorld, port: i32, volts: f32) {
    let dc = world.inverter.dc.as_ref().expect("dc mapped");
    let p = dc
        .external_measurement
        .iter()
        .find(|p| p.port_number == port)
        .unwrap_or_else(|| panic!("no DC port {port}"));
    assert!(
        approx(p.line[0].voltage, volts),
        "DC port {port} voltage {} !~ {volts}",
        p.line[0].voltage
    );
}

#[then(expr = "DC port {int} amperage is approximately {float} A")]
fn then_dc_port_amperage(world: &mut SpecWorld, port: i32, amps: f32) {
    let dc = world.inverter.dc.as_ref().expect("dc mapped");
    let p = dc
        .external_measurement
        .iter()
        .find(|p| p.port_number == port)
        .unwrap_or_else(|| panic!("no DC port {port}"));
    assert!(
        approx(p.line[0].amperage, amps),
        "DC port {port} amperage {} !~ {amps}",
        p.line[0].amperage
    );
}

#[then(expr = "the power electronics temperature is approximately {float} C")]
fn then_hw_temperature(world: &mut SpecWorld, celsius: f32) {
    let hw = world.inverter.hw.as_ref().expect("hw mapped");
    let pe = hw
        .power_electronics
        .as_ref()
        .expect("power_electronics present");
    assert!(
        approx(pe.temperature, celsius),
        "temp {} !~ {celsius}",
        pe.temperature
    );
}

#[then("there is no energy storage state")]
fn then_hw_no_storage(world: &mut SpecWorld) {
    let hw = world.inverter.hw.as_ref().expect("hw mapped");
    assert!(
        hw.energy_storage.is_none(),
        "a converter has no energy_storage state"
    );
}

#[then(expr = "there is a diagnostic with SPN {int}")]
fn then_diag_has_spn(world: &mut SpecWorld, spn: u32) {
    let diag = world.inverter.diag.as_ref().expect("diag mapped");
    assert!(
        diag.codes.iter().any(|c| c.suspect_parameter == spn),
        "expected a DTC with SPN {spn}; got {:?}",
        diag.codes
            .iter()
            .map(|c| c.suspect_parameter)
            .collect::<Vec<_>>()
    );
}

#[then(expr = "the diagnostic count is {int}")]
fn then_diag_count(world: &mut SpecWorld, n: usize) {
    let diag = world.inverter.diag.as_ref().expect("diag mapped");
    assert_eq!(diag.codes.len(), n);
}

#[then("every diagnostic is Major severity with a ConditionExists failure mode")]
fn then_diag_major_condition_exists(world: &mut SpecWorld) {
    let diag = world.inverter.diag.as_ref().expect("diag mapped");
    for c in diag.codes.iter() {
        assert!(
            matches!(c.severity, DtcSeverity::Major),
            "severity must be Major"
        );
        assert!(
            matches!(c.failure_mode, FailureModeIndicator::ConditionExists),
            "failure mode must be ConditionExists"
        );
    }
}

#[then("every inverter SPN is disjoint from the battery SPN range")]
fn then_diag_spns_disjoint(world: &mut SpecWorld) {
    let diag = world.inverter.diag.as_ref().expect("diag mapped");
    // Battery SPNs are 1..=16 (see lib_mapping::transform::FAULT_TABLE); inverter SPNs are >= 100.
    assert!(
        diag.codes.iter().all(|c| c.suspect_parameter >= 100),
        "inverter SPNs must not collide with the battery 1..=16 range"
    );
}
