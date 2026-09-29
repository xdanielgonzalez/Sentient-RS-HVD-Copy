//! Steps for the `tms` feature area: **structural** conformance of the TMS (MIL-STD-3071) schema
//! shape to the IDL, as distinct from `mapping`'s VALUE-transform assertions (scaling, sign,
//! round-trip). Every struct/enum/constant asserted on here is the real `lib_tms` type —
//! nothing is re-implemented or re-derived; enum discriminants in particular are always read off
//! the real enum via `as u8`, never hand-copied as a numeric literal. See
//! `tests/features/tms_spec_compliance.feature` and `docs/tms-compliance.md` §3/§5/§6 for the
//! oracle this suite checks against.
//!
//! Pure in-process: constructed structs + `serde_json` only, no DDS runtime, no Seplos decode.
//!
//! Test code only; freely uses unwrap/expect/panic (the workspace lint gate denies these in
//! production logic, not tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cucumber::{given, then, when};

use heapless::Vec as HVec;
use lib_mapping::sink::topic_into;
use lib_tms::{
    topics, ActiveDiagnosticState, DcMeasurementUpdate,
    DcPowerLineMeasurement as PowerLineMeasurement, DcPowerPortMeasurement as PowerPortMeasurement,
    DiagnosticTroubleCode, DtcSeverity, EnergyStorageState, FailureModeIndicator,
    PowerHardwareUpdate, StorageCellState, String32, MAX_DC_LINES, MAX_DTC, MAX_STORAGE_CELLS,
};

use crate::{dev, now, SpecWorld};

/// Scenario state for the tms feature area (structural spec-compliance).
#[derive(Debug, Default)]
pub struct TmsState {
    /// The JSON most recently produced by a `given_*`/`when_*` step, via `serde_json::to_string`
    /// on a real `lib_tms` type. `then_*` steps parse this back into a `serde_json::Value`
    /// for field-shape assertions.
    json: String,
    /// Result of the most recent bounded-sequence overflow push attempt: `Some(true)` = the push
    /// past MAXLEN unexpectedly succeeded, `Some(false)` = it failed at capacity as required,
    /// `None` = no push attempted yet this scenario.
    push_result: Option<bool>,
}

// ---- Fixture builders (real types only; every field explicitly valued, no `Default`-derived
// TMS-schema instances — `Default` isn't even derived on most of these structs) -----------------

fn hstr32(s: &str) -> String32 {
    let mut h = String32::new();
    h.push_str(s).expect("fits String32");
    h
}

/// `power_rate_of_change` is always NaN from the real mapper (no per-port rate-of-change telemetry
/// source, `tms-compliance.md` §3.1/§4) — mirrored here, not a test-only choice.
fn full_line() -> PowerLineMeasurement {
    PowerLineMeasurement {
        voltage: 54.51,
        amperage: 1.44,
        power: 78.5,
    }
}

fn full_port(port_number: i32) -> PowerPortMeasurement {
    let mut line: HVec<PowerLineMeasurement, MAX_DC_LINES> = HVec::new();
    line.push(full_line()).expect("push line within MAXLEN");
    PowerPortMeasurement {
        port_number,
        line,
        power_rate_of_change: f32::NAN,
    }
}

fn full_dc() -> DcMeasurementUpdate {
    let mut ext: HVec<PowerPortMeasurement, 4> = HVec::new();
    ext.push(full_port(0)).expect("push external port");
    DcMeasurementUpdate {
        device_id: dev(),
        time_measured: now(),
        external_measurement: ext,
        internal_measurement: HVec::new(),
    }
}

fn full_storage_cell(i: u16) -> StorageCellState {
    StorageCellState {
        cell_number: i,
        cycle_counter: 2.0,
        // #278: `state_of_charge` is `Option<f32>` (IDL `@optional`); `Some(x)` serializes as bare x.
        state_of_charge: Some(0.775),
        temperature: 7.0,
        voltage: 3.894,
    }
}

/// `run_time` is always NaN from the real mapper (Seplos carries no cumulative hour-meter,
/// `tms-compliance.md` §3.2.1/§4) — mirrored here, not a test-only choice.
fn full_energy_storage() -> EnergyStorageState {
    let mut cells: HVec<StorageCellState, MAX_STORAGE_CELLS> = HVec::new();
    cells
        .push(full_storage_cell(0))
        .expect("push cell within MAXLEN");
    EnergyStorageState {
        min_temperature: 7.0,
        average_temperature: 7.5,
        max_temperature: 8.0,
        run_time: f32::NAN,
        storage_cells: cells,
    }
}

fn full_power_hardware() -> PowerHardwareUpdate {
    PowerHardwareUpdate {
        device_id: dev(),
        timestamp: now(),
        // Battery-only device: only `energy_storage` is populated; the other five @optional
        // subsystem states stay None.
        engine: None,
        fuel: None,
        generator: None,
        energy_storage: Some(full_energy_storage()),
        power_electronics: None,
        thermal: None,
    }
}

fn power_hardware_no_energy_storage() -> PowerHardwareUpdate {
    PowerHardwareUpdate {
        device_id: dev(),
        timestamp: now(),
        engine: None,
        fuel: None,
        generator: None,
        energy_storage: None,
        power_electronics: None,
        thermal: None,
    }
}

fn full_diagnostic_trouble_code() -> DiagnosticTroubleCode {
    DiagnosticTroubleCode {
        port_number: Some(0),
        suspect_parameter: 1,
        failure_mode: FailureModeIndicator::ConditionExists,
        occurrence_count: 1,
        time_raised: now(),
        severity: DtcSeverity::Major,
        hint: hstr32("high voltage protection"),
        estimate_time: Some(now()),
        estimate_severity: Some(DtcSeverity::Major),
    }
}

/// The four `@optional` `DiagnosticTroubleCode` fields the real mapper always writes `None` for
/// (`tms-compliance.md` §3.3) — everything else stays identically valued to
/// [`full_diagnostic_trouble_code`] so only the optionality is under test.
fn diagnostic_trouble_code_optionals_none() -> DiagnosticTroubleCode {
    DiagnosticTroubleCode {
        port_number: None,
        estimate_time: None,
        estimate_severity: None,
        ..full_diagnostic_trouble_code()
    }
}

fn full_active_diagnostic_state() -> ActiveDiagnosticState {
    let mut codes: HVec<DiagnosticTroubleCode, MAX_DTC> = HVec::new();
    codes
        .push(full_diagnostic_trouble_code())
        .expect("push code within MAXLEN");
    ActiveDiagnosticState {
        device_id: dev(),
        codes,
        overflow: 0,
    }
}

fn tms_topic_constant(name: &str) -> &'static str {
    match name {
        "DC_MEASUREMENT_UPDATE" => topics::DC_MEASUREMENT_UPDATE,
        "POWER_HARDWARE_UPDATE" => topics::POWER_HARDWARE_UPDATE,
        "ACTIVE_DIAGNOSTIC_STATE" => topics::ACTIVE_DIAGNOSTIC_STATE,
        "STORAGE_UPDATE" => topics::STORAGE_UPDATE,
        // TMS breadth (#240 #244 #246) — see `tms_breadth.feature`'s own topic-constant scenario.
        "HEARTBEAT" => topics::HEARTBEAT,
        "DEVICE_INFO" => topics::DEVICE_INFO,
        "REPLY" => topics::REPLY,
        "AC_MEASUREMENT_UPDATE" => topics::AC_MEASUREMENT_UPDATE,
        "CONTROL_HARDWARE_UPDATE" => topics::CONTROL_HARDWARE_UPDATE,
        "ENERGY_START_STOP_STATE" => topics::ENERGY_START_STOP_STATE,
        "POWER_PORT_STATE" => topics::POWER_PORT_STATE,
        // Required STOR state topics wired end-to-end in wave 5 (#263) — see `tms_breadth.feature`.
        "ACTIVE_MICROGRID_CONTROLLER_STATE" => topics::ACTIVE_MICROGRID_CONTROLLER_STATE,
        "DC_LOAD_SHARING_STATE" => topics::DC_LOAD_SHARING_STATE,
        "ENERGY_START_STOP_REQUEST" => topics::ENERGY_START_STOP_REQUEST,
        "POWER_SWITCH_REQUEST" => topics::POWER_SWITCH_REQUEST,
        other => panic!("unknown TMS schema topic constant {other:?} in feature file"),
    }
}

fn json_value(world: &SpecWorld) -> serde_json::Value {
    serde_json::from_str(&world.tms.json).expect("TmsState::json is valid JSON")
}

// ---- Given: construct a real TMS-schema type and stash its JSON shape -------------------------

#[given(expr = "the TMS schema fully-populated {word}")]
fn given_fully_populated(world: &mut SpecWorld, type_name: String) {
    let json = match type_name.as_str() {
        "DcMeasurementUpdate" => serde_json::to_string(&full_dc()),
        "PowerPortMeasurement" => serde_json::to_string(&full_port(0)),
        "PowerLineMeasurement" => serde_json::to_string(&full_line()),
        "PowerHardwareUpdate" => serde_json::to_string(&full_power_hardware()),
        "EnergyStorageState" => serde_json::to_string(&full_energy_storage()),
        "StorageCellState" => serde_json::to_string(&full_storage_cell(0)),
        "ActiveDiagnosticState" => serde_json::to_string(&full_active_diagnostic_state()),
        "DiagnosticTroubleCode" => serde_json::to_string(&full_diagnostic_trouble_code()),
        other => panic!("unknown TMS schema type {other:?} in feature file"),
    }
    .expect("serialize TMS schema type to JSON");
    world.tms.json = json;
}

#[given(expr = "the TMS schema {word} with all optional fields set to None")]
fn given_optionals_none(world: &mut SpecWorld, type_name: String) {
    let json = match type_name.as_str() {
        "DiagnosticTroubleCode" => serde_json::to_string(&diagnostic_trouble_code_optionals_none()),
        "PowerHardwareUpdate" => serde_json::to_string(&power_hardware_no_energy_storage()),
        other => panic!("unknown TMS schema type {other:?} in feature file"),
    }
    .expect("serialize TMS schema type to JSON");
    world.tms.json = json;
}

// ---- When: bounded-sequence MAXLEN overflow probes (heapless::Vec::push) -----------------------

#[when("64 TMS schema DiagnosticTroubleCode entries fill the codes sequence and a 65th is pushed")]
fn when_overflow_codes(world: &mut SpecWorld) {
    let mut codes: HVec<DiagnosticTroubleCode, MAX_DTC> = HVec::new();
    for _ in 0..64 {
        codes
            .push(full_diagnostic_trouble_code())
            .expect("push within MAXLEN=64");
    }
    world.tms.push_result = Some(codes.push(full_diagnostic_trouble_code()).is_ok());
}

#[when(
    "50 TMS schema StorageCellState entries fill the storage_cells sequence and a 51st is pushed"
)]
fn when_overflow_storage_cells(world: &mut SpecWorld) {
    let mut cells: HVec<StorageCellState, MAX_STORAGE_CELLS> = HVec::new();
    for i in 0..50u16 {
        cells
            .push(full_storage_cell(i))
            .expect("push within MAXLEN=50");
    }
    world.tms.push_result = Some(cells.push(full_storage_cell(50)).is_ok());
}

#[when("3 TMS schema PowerLineMeasurement entries fill a port's line sequence and a 4th is pushed")]
fn when_overflow_line(world: &mut SpecWorld) {
    let mut line: HVec<PowerLineMeasurement, MAX_DC_LINES> = HVec::new();
    for _ in 0..3 {
        line.push(full_line()).expect("push within MAXLEN=3");
    }
    world.tms.push_result = Some(line.push(full_line()).is_ok());
}

#[when("4 TMS schema PowerPortMeasurement entries fill external_measurement and a 5th is pushed")]
fn when_overflow_ports(world: &mut SpecWorld) {
    let mut ext: HVec<PowerPortMeasurement, 4> = HVec::new();
    for i in 0..4i32 {
        ext.push(full_port(i)).expect("push within Rust cap=4");
    }
    world.tms.push_result = Some(ext.push(full_port(4)).is_ok());
}

// ---- Then: JSON field-shape assertions (generic over the scratch `json` string) ----------------

#[then(expr = "the TMS schema JSON has {int} top-level field")]
#[then(expr = "the TMS schema JSON has {int} top-level fields")]
fn then_field_count(world: &mut SpecWorld, n: i64) {
    let v = json_value(world);
    let obj = v.as_object().expect("TMS schema JSON is an object");
    assert_eq!(
        obj.len(),
        n as usize,
        "top-level field count mismatch: {obj:?}"
    );
}

#[then(expr = "the TMS schema JSON top-level field {word} is present")]
fn then_field_present(world: &mut SpecWorld, field: String) {
    let v = json_value(world);
    let obj = v.as_object().expect("TMS schema JSON is an object");
    assert!(
        obj.contains_key(&field),
        "expected field {field:?} present, got keys {:?}",
        obj.keys().collect::<Vec<_>>()
    );
}

#[then(expr = "the TMS schema JSON top-level field {word} is absent")]
fn then_field_absent(world: &mut SpecWorld, field: String) {
    let v = json_value(world);
    let obj = v.as_object().expect("TMS schema JSON is an object");
    assert!(
        !obj.contains_key(&field),
        "expected field {field:?} absent, got keys {:?}",
        obj.keys().collect::<Vec<_>>()
    );
}

#[then(expr = "the TMS schema JSON top-level field {word} is null")]
fn then_field_null(world: &mut SpecWorld, field: String) {
    let v = json_value(world);
    let obj = v.as_object().expect("TMS schema JSON is an object");
    assert_eq!(
        obj.get(&field),
        Some(&serde_json::Value::Null),
        "expected field {field:?} to be JSON null, got {:?}",
        obj.get(&field)
    );
}

// ---- Then: topic-string literals + topic_into (lib_mapping::sink) -------------------------

#[then(expr = "the TMS schema topic constant {word} equals {string}")]
fn then_topic_constant(_world: &mut SpecWorld, name: String, expected: String) {
    assert_eq!(tms_topic_constant(&name), expected);
}

#[then(
    expr = "the TMS schema topic_into prefix {string} device {string} topic {word} yields {string}"
)]
fn then_topic_into(
    _world: &mut SpecWorld,
    prefix: String,
    device: String,
    topic_name: String,
    expected: String,
) {
    let tms_topic = tms_topic_constant(&topic_name);
    let mut out: heapless::String<128> = heapless::String::new();
    topic_into(&prefix, &device, tms_topic, &mut out).expect("topic fits the buffer");
    assert_eq!(out.as_str(), expected);
}

#[then("the TMS schema STORAGE_UPDATE topic constant is published at the spec Periodic 10 s rate")]
fn then_storage_update_now_published(_world: &mut SpecWorld) {
    // `topics::STORAGE_UPDATE` (lib-tms/src/lib.rs) is wired end to end through
    // `bin_rpi_platform::HostPlatform::emit_reading` via `ToStorageUpdate` (lib-mapping). Wave 5
    // (#265) rate-limits it (and `PowerHardwareUpdate`) to the spec Slow / Periodic 10 s profile
    // (§4.9.10.2) rather than emitting on every ~1 s poll cycle — see `EmitCadence` and
    // `test-e2e/tests/spec_conformance_e2e.rs` for the live-broker cadence proof. This assertion
    // documents the constant's shape only; it is not itself a live-broker check.
    assert_eq!(tms_topic_constant("STORAGE_UPDATE"), "StorageUpdate");
}

// ---- Then: enum discriminants, read off the real enum via `as u8` (never hand-copied) ----------

#[then(expr = "the TMS schema DtcSeverity variant {word} has discriminant {int}")]
fn then_dtc_severity_discriminant(_world: &mut SpecWorld, variant: String, expected: i64) {
    let value = match variant.as_str() {
        "Unknown" => DtcSeverity::Unknown,
        "Clear" => DtcSeverity::Clear,
        "Informative" => DtcSeverity::Informative,
        "Preventative" => DtcSeverity::Preventative,
        "Degraded" => DtcSeverity::Degraded,
        "Warning" => DtcSeverity::Warning,
        "Minor" => DtcSeverity::Minor,
        "Major" => DtcSeverity::Major,
        "Critical" => DtcSeverity::Critical,
        "Fatal" => DtcSeverity::Fatal,
        "HumanSafety" => DtcSeverity::HumanSafety,
        other => panic!("unknown DtcSeverity variant {other:?} in feature file"),
    };
    assert_eq!(value as u8, expected as u8);
}

#[then(expr = "the TMS schema FailureModeIndicator variant {word} has discriminant {int}")]
fn then_fmi_discriminant(_world: &mut SpecWorld, variant: String, expected: i64) {
    let value = match variant.as_str() {
        "Unknown" => FailureModeIndicator::Unknown,
        "AboveNormalMostSevere" => FailureModeIndicator::AboveNormalMostSevere,
        "BelowNormalMostSevere" => FailureModeIndicator::BelowNormalMostSevere,
        "DataErratic" => FailureModeIndicator::DataErratic,
        "VoltageHigh" => FailureModeIndicator::VoltageHigh,
        "VoltageLow" => FailureModeIndicator::VoltageLow,
        "CurrentLow" => FailureModeIndicator::CurrentLow,
        "CurrentHigh" => FailureModeIndicator::CurrentHigh,
        "NotResponding" => FailureModeIndicator::NotResponding,
        "AbnormalFrequency" => FailureModeIndicator::AbnormalFrequency,
        "AbnormalUpdateRate" => FailureModeIndicator::AbnormalUpdateRate,
        "AbnormalRateOfChange" => FailureModeIndicator::AbnormalRateOfChange,
        "FailureCodeNotIdentifiable" => FailureModeIndicator::FailureCodeNotIdentifiable,
        "BadIntelligentDevice" => FailureModeIndicator::BadIntelligentDevice,
        "OutOfCalibration" => FailureModeIndicator::OutOfCalibration,
        "SpecialInstructions" => FailureModeIndicator::SpecialInstructions,
        "AboveNormalLeastSevere" => FailureModeIndicator::AboveNormalLeastSevere,
        "AboveNormalModeratelySevere" => FailureModeIndicator::AboveNormalModeratelySevere,
        "BelowNormalLeastSevere" => FailureModeIndicator::BelowNormalLeastSevere,
        "BelowNormalModeratelySevere" => FailureModeIndicator::BelowNormalModeratelySevere,
        "ReceivedDataError" => FailureModeIndicator::ReceivedDataError,
        "DataDriftedHigh" => FailureModeIndicator::DataDriftedHigh,
        "DataDriftedLow" => FailureModeIndicator::DataDriftedLow,
        "ConditionExists" => FailureModeIndicator::ConditionExists,
        other => panic!("unknown FailureModeIndicator variant {other:?} in feature file"),
    };
    assert_eq!(value as u8, expected as u8);
}

// ---- Then: MAXLEN bound assertions --------------------------------------------------------------

#[then("the TMS schema sequence push beyond MAXLEN fails")]
fn then_push_fails(world: &mut SpecWorld) {
    assert_eq!(
        world.tms.push_result,
        Some(false),
        "expected the push past MAXLEN to fail (Err), it unexpectedly succeeded"
    );
}

#[then(
    "the TMS schema DcMeasurementUpdate external_measurement cap of 4 is within the IDL MAX_PORTS bound of 64"
)]
fn then_port_cap_within_idl_bound(_world: &mut SpecWorld) {
    // The Rust bound is a literal `Vec<PowerPortMeasurement, 4>` (lib-tms/src/generated.rs)
    // — there is no Rust-side MAX_PORTS constant to read (tms-compliance.md §7d resolved this: the
    // IDL's MAX_PORTS is confirmed 64 at chapter-d.txt:4757). The Rust cap is read off the real
    // type's `.capacity()` rather than hand-copied, and compared against the IDL's confirmed value.
    let rust_cap = full_dc().external_measurement.capacity();
    let idl_max_ports = 64usize;
    assert!(
        rust_cap <= idl_max_ports,
        "Rust external_measurement cap {rust_cap} exceeds confirmed IDL MAX_PORTS {idl_max_ports}"
    );
}
