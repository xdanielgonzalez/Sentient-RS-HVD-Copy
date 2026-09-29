//! CROSS-ORACLE TEST — the permanent gate asserting the flat wire serializer (`src/wire`, reached
//! via [`lib_mapping::render`]'s `render_*` fns) and the serde reference path (the cfg-gated
//! `transform`/`transform_inverter` materializers + [`lib_mapping::codec::to_json_slice`]) produce
//! the SAME JSON shape for the same canonical input — for every rendered topic: battery
//! Dc/PowerHardware/ActiveDiagnostic, inverter Ac/Dc/PowerHardware/ActiveDiagnostic, and
//! StorageUpdate.
//!
//! GROUND TRUTH: both paths must agree with the IDL declaration order
//! (`crates/lib-tms/src/generated.rs`, sourced from `docs/compliance/tms/mil-std-3071_data_model.idl`)
//! and the omit-absent policy (MIL-STD-3071 A.9.7.5 / Table A.13: an absent `@optional` member is
//! "Empty (no contained value)" — the key is omitted, not nulled). The generated structs carry
//! `#[serde(skip_serializing_if = "Option::is_none", default)]`, so the serde walk and the generated
//! field chains enforce the same shape from the same source; any disagreement is a binding bug.
//!
//! HISTORY: this file began as the characterization test that pinned the two pre-flip serializer
//! bugs — (A) DTC field order hoisting `hint` (issue #273) and (B) `PowerHardwareUpdate` emitting
//! the five absent subsystems as `null` on the serde side. Both were fixed by the field-chain flip;
//! the rows that once `#[ignore]`d-failed now run un-ignored and MUST PASS — that's the gate this
//! file backs.
//!
//! ORDER SENSITIVITY CAVEAT: [`compare_shape`]'s "key list EXACTLY equal, order AND presence" check
//! is only as order-sensitive as the `serde_json::Value` object representation underneath it —
//! `serde_json`'s default `Map` is a `BTreeMap` (alphabetically sorted on parse, order information
//! discarded), which would silently HIDE divergence (A) (same key SET, different source order) while
//! still catching (B) (different key SET, order-independent). Order-sensitivity requires
//! `serde_json`'s `preserve_order` feature (backed by `indexmap`, preserving the parsed source
//! order) on this crate's `dev-dependencies.serde_json`. See this WP's report for whether that
//! feature was present when this file was last run.
//!
//! Fixture builders (`dev`/`now`/`reading`/`fault`) are copied verbatim from
//! `tests/wire_byte_identity.rs` (not shared — each integration-test binary is its own crate root,
//! and this repo has no shared test-support crate for `lib-mapping`'s `tests/`); the `LIVE_FRAME`
//! golden + tolerance constant are copied from `tests/render_tms.rs`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_mapping::codec::to_json_slice;
use lib_mapping::render::{render_fault, render_tms, render_value, TmsTopic};
use lib_mapping::schema::{
    ActiveMicrogridControllerState, ConfigId, DcLoadSharingState, DeviceInfo, DeviceRole,
    EnergyStartStopLevel, EnergyStartStopState, Heartbeat, PowerPortState, PowerPortStatus,
    PowerSwitchReason, PowerSwitchTransitionCode, Reply, ReplyCode, ReplyStatus,
};
use lib_mapping::transform::{ToDiagnostics, ToStorageUpdate, ToTms};
use lib_parse::canonical::{CanonicalFault, CanonicalReading};
use lib_parse::seplos::Seplos;
use lib_parse::vendor::BmsVendorProtocol;
use lib_tms::{ClockMonotonic, Identity};
use serde_json::Value;

/// TMS accuracy tolerance for numeric comparison (`docs/tms-compliance.md`), shared with
/// `tests/render_tms.rs`.
const TOL: f64 = 0.005;

/// The real Seplos frame `tests/render_tms.rs` decodes its goldens from — reused here so the
/// oracle exercises one live-shaped fixture alongside the synthetic sweep.
const LIVE_FRAME: &[u8] = b"~20004600A08E00000E0F360F360F360F360F350F360F360F360F360F370F370F360F360F35060AF10AF90AF90AF30B370B0CFF5C154C36820A465003074650000203E8154CFFF800000079004DDEC8\r";

// ---- fixture builders (copied verbatim from tests/wire_byte_identity.rs) ----

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

/// The real `LIVE_FRAME` fixture, decoded — reused from `tests/render_tms.rs`.
fn live_reading() -> CanonicalReading {
    Seplos::new()
        .parse_reading(LIVE_FRAME, 0)
        .expect("decode LIVE_FRAME")
}

// ---- comparator ----

fn kind_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Recursive shape walk backing [`compare_shape`]. `path` is a JSON-Pointer-style path (RFC 6901
/// shape, minus percent-escaping — none of this schema's keys contain `/` or `~`) rooted at `""`;
/// every divergence found is appended to `errors` rather than short-circuiting, so one call reports
/// every distinct divergence in the tree, not just the first.
fn compare_at(expected: &Value, actual: &Value, path: &str, errors: &mut Vec<String>) {
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => {
            let e_keys: Vec<&str> = e.keys().map(String::as_str).collect();
            let a_keys: Vec<&str> = a.keys().map(String::as_str).collect();
            if e_keys != a_keys {
                errors.push(format!(
                    "{path}: object key list differs (order and/or presence)\n    expected (serde): {e_keys:?}\n    actual   (wire):  {a_keys:?}"
                ));
            }
            // Recurse into whatever both sides actually share, so a presence divergence at this
            // level doesn't hide a further divergence nested inside a shared key.
            for key in e.keys() {
                if let Some(av) = a.get(key) {
                    compare_at(&e[key], av, &format!("{path}/{key}"), errors);
                }
            }
        }
        (Value::Array(e), Value::Array(a)) => {
            if e.len() != a.len() {
                errors.push(format!(
                    "{path}: array length differs: expected {} actual {}",
                    e.len(),
                    a.len()
                ));
            }
            for (i, (ev, av)) in e.iter().zip(a.iter()).enumerate() {
                compare_at(ev, av, &format!("{path}/{i}"), errors);
            }
        }
        (Value::Number(e), Value::Number(a)) => match (e.as_f64(), a.as_f64()) {
            (Some(ef), Some(af)) => {
                if (ef - af).abs() > TOL {
                    errors.push(format!(
                        "{path}: number differs beyond tolerance: expected {ef} actual {af} (tol {TOL})"
                    ));
                }
            }
            _ => errors.push(format!(
                "{path}: number not representable as f64: expected {e} actual {a}"
            )),
        },
        (Value::String(e), Value::String(a)) => {
            if e != a {
                errors.push(format!(
                    "{path}: string differs: expected {e:?} actual {a:?}"
                ));
            }
        }
        (Value::Bool(e), Value::Bool(a)) => {
            if e != a {
                errors.push(format!("{path}: bool differs: expected {e} actual {a}"));
            }
        }
        (Value::Null, Value::Null) => {}
        _ => errors.push(format!(
            "{path}: type mismatch: expected {} actual {}",
            kind_name(expected),
            kind_name(actual)
        )),
    }
}

/// Tier 1 — shape comparator. `expected` is the serde reference, `actual` is the wire output (this
/// ordering is load-bearing for the error messages' "expected (serde)"/"actual (wire)" labels).
///
/// Recursive: objects require EXACTLY equal key lists (order AND presence — meaningful only under
/// `serde_json`'s `preserve_order` feature, see this file's module doc); arrays require equal length
/// with element-wise comparison; numbers compare within `TOL` (0.005); strings/bools/null compare
/// exactly; a type mismatch (e.g. `null` vs a number) is its own divergence. Returns every distinct
/// divergence found (empty = the two shapes match); each entry embeds a JSON-Pointer-style path.
fn compare_shape(expected: &Value, actual: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    compare_at(expected, actual, "", &mut errors);
    errors
}

/// Tier 2 — exact byte comparator. UNUSED this phase (kept for later phases' identity topics, where
/// wire and serde are expected to produce byte-identical output, not merely same-shape).
#[allow(dead_code)]
fn exact_bytes(expected: &[u8], actual: &[u8]) {
    assert_eq!(expected, actual, "byte-identical wire vs serde reference");
}

// ---- render helpers: wire path vs serde reference path, each parsed to a Value ----

fn wire_dc(reading: &CanonicalReading, device_id: &Identity, t: ClockMonotonic) -> Value {
    let mut buf = [0u8; 8192];
    let n = render_tms(
        TmsTopic::DcMeasurementUpdate,
        device_id.clone(),
        reading,
        t,
        &mut buf,
    )
    .expect("render_tms(DcMeasurementUpdate)");
    serde_json::from_slice(&buf[..n]).expect("wire dc_measurement is valid JSON")
}

fn serde_dc(reading: &CanonicalReading, device_id: &Identity, t: ClockMonotonic) -> Value {
    let payload = reading.to_dc_measurement(device_id.clone(), t);
    let mut buf = [0u8; 8192];
    let n = to_json_slice(&payload, &mut buf).expect("to_json_slice(DcMeasurementUpdate)");
    serde_json::from_slice(&buf[..n]).expect("serde dc_measurement is valid JSON")
}

fn wire_hw(reading: &CanonicalReading, device_id: &Identity, t: ClockMonotonic) -> Value {
    let mut buf = [0u8; 8192];
    let n = render_tms(
        TmsTopic::PowerHardwareUpdate,
        device_id.clone(),
        reading,
        t,
        &mut buf,
    )
    .expect("render_tms(PowerHardwareUpdate)");
    serde_json::from_slice(&buf[..n]).expect("wire power_hardware is valid JSON")
}

fn serde_hw(reading: &CanonicalReading, device_id: &Identity, t: ClockMonotonic) -> Value {
    let payload = reading.to_power_hardware(device_id.clone(), t);
    let mut buf = [0u8; 8192];
    let n = to_json_slice(&payload, &mut buf).expect("to_json_slice(PowerHardwareUpdate)");
    serde_json::from_slice(&buf[..n]).expect("serde power_hardware is valid JSON")
}

fn wire_diag(fault: &CanonicalFault, device_id: &Identity, t: ClockMonotonic) -> Value {
    let mut buf = [0u8; 8192];
    let n = render_fault(fault, device_id.clone(), t, &mut buf).expect("render_fault");
    serde_json::from_slice(&buf[..n]).expect("wire active_diagnostic is valid JSON")
}

fn serde_diag(fault: &CanonicalFault, device_id: &Identity, t: ClockMonotonic) -> Value {
    let payload = fault.to_active_diagnostics(device_id.clone(), t);
    let mut buf = [0u8; 8192];
    let n = to_json_slice(&payload, &mut buf).expect("to_json_slice(ActiveDiagnosticState)");
    serde_json::from_slice(&buf[..n]).expect("serde active_diagnostic is valid JSON")
}

/// Run `compare_shape` for one fixture row and fold every divergence into `report`, prefixed with
/// `label` so a multi-row test's failure output identifies which row each divergence came from.
fn fold(report: &mut Vec<String>, label: &str, expected: &Value, actual: &Value) {
    for e in compare_shape(expected, actual) {
        report.push(format!("[{label}] {e}"));
    }
}

// ---- battery: dc_measurement ----

/// `DcMeasurementUpdate` — wire vs serde, across the same (current, voltage) sweep
/// `tests/wire_byte_identity.rs::dc_measurement_encoder_is_byte_identical` uses, plus the live frame.
///
/// EXPECTED (today): no divergence — `DcMeasurementUpdate`/`DcPowerPortMeasurement`/
/// `DcPowerLineMeasurement`'s field order is identical on both paths, and `NaN`
/// (`power_rate_of_change`) renders `null` on both (`serde_json_core::serialize_f32` -> `null` for
/// non-finite, matching `JsonSink::null`). This test is bundled with the other two under one
/// `#[ignore]` reason for file-level consistency, not because it characterizes a bug of its own.
#[test]
fn dc_measurement() {
    let d = dev("METEOR-001");
    let t = now(12345, 678_000_000);
    let mut report = Vec::new();

    for &(current, volt) in &[
        (-164i16, 5452u16),
        (250, 5452),
        (0, 0),
        (-1, 1),
        (32000, 60000),
    ] {
        let label = format!("dc_measurement/current={current}/volt={volt}");
        let r = reading(14, 6, current, volt, 775, 2);
        fold(
            &mut report,
            &label,
            &serde_dc(&r, &d, t),
            &wire_dc(&r, &d, t),
        );
    }

    let r = live_reading();
    fold(
        &mut report,
        "dc_measurement/LIVE_FRAME",
        &serde_dc(&r, &d, t),
        &wire_dc(&r, &d, t),
    );

    assert!(
        report.is_empty(),
        "dc_measurement wire/serde shape divergence:\n{}",
        report.join("\n")
    );
}

// ---- battery: power_hardware ----

/// `PowerHardwareUpdate` — wire vs serde, across the cells x temps sweep
/// `tests/wire_byte_identity.rs::power_hardware_encoder_is_byte_identical` uses (soc/cycles held at
/// one representative pair — the sweep's soc/cycles axis exercises wire/wire byte-identity, not
/// wire/serde shape, so widening it here would not surface any additional divergence), plus the live
/// frame.
///
/// EXPECTED (today): every row fails with EXACTLY divergence (B) — the top-level object key list
/// differs (`engine`/`fuel`/`generator`/`power_electronics`/`thermal` present-as-`null` on serde,
/// absent on wire). The shared `energy_storage` subtree (min/average/max temperature, run_time,
/// storage_cells[*]) is otherwise identical field-for-field, so once the two sides share the same
/// key list, no further divergence is expected there.
#[test]
fn power_hardware() {
    let d = dev("METEOR-001");
    let t = now(12345, 678_000_000);
    let mut report = Vec::new();

    for &cells in &[0usize, 1, 14, 16] {
        for &temps in &[0usize, 1, 6, 8] {
            let label = format!("power_hardware/cells={cells}/temps={temps}");
            let r = reading(cells, temps, -164, 5452, 775, 2);
            fold(
                &mut report,
                &label,
                &serde_hw(&r, &d, t),
                &wire_hw(&r, &d, t),
            );
        }
    }

    let r = live_reading();
    fold(
        &mut report,
        "power_hardware/LIVE_FRAME",
        &serde_hw(&r, &d, t),
        &wire_hw(&r, &d, t),
    );

    assert!(
        report.is_empty(),
        "power_hardware wire/serde shape divergence:\n{}",
        report.join("\n")
    );
}

// ---- fault: active_diagnostic ----

/// `ActiveDiagnosticState` — wire vs serde, across a representative slice of the exhaustive
/// `0..=0xFFFF` sweep `tests/wire_byte_identity.rs::active_diagnostic_encoder_is_byte_identical`
/// covers in full (full-exhaustive is a wire/wire byte-identity concern; wire/serde SHAPE divergence
/// is per-DTC and does not vary across which flag is set, so a representative slice is sufficient
/// here): zero flags, one flag, several flags, and all sixteen flags.
///
/// EXPECTED (today): the zero-flags row (`codes: []`) has no DTC to diverge on and passes clean.
/// Every row with at least one flag set fails with EXACTLY divergence (A), once per emitted DTC —
/// the wire encoder orders `hint` before `estimate_time`/`estimate_severity`; the struct/serde order
/// is `estimate_time`, `estimate_severity`, `hint` last. All other DTC fields (`port_number`,
/// `suspect_parameter`, `failure_mode`, `occurrence_count`, `time_raised`, `severity`) match.
#[test]
fn active_diagnostic() {
    let d = dev("METEOR-001");
    let t = now(12345, 678_000_000);
    let mut report = Vec::new();

    // 0 flags, 1 flag, several flags, all 16 flags.
    for &(label, bits) in &[
        ("zero", 0u32),
        ("one", 0b0000_0000_0000_0001u32),
        ("several", 0b0000_0000_1010_1010u32),
        ("all16", 0b1111_1111_1111_1111u32),
    ] {
        let row_label = format!("active_diagnostic/{label}(bits={bits:#06x})");
        let f = fault(bits);
        fold(
            &mut report,
            &row_label,
            &serde_diag(&f, &d, t),
            &wire_diag(&f, &d, t),
        );
    }

    assert!(
        report.is_empty(),
        "active_diagnostic wire/serde shape divergence:\n{}",
        report.join("\n")
    );
}

// ---- inverter: ac_measurement ----

/// Helper: build representative CanonicalInverterReading fixture.
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
) -> lib_parse::canonical::CanonicalInverterReading {
    lib_parse::canonical::CanonicalInverterReading {
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
        mode: lib_parse::canonical::InverterMode::Line,
        ratings: None,
    }
}

/// Helper: build representative CanonicalInverterFault fixture.
fn inverter_fault(
    line_fail: bool,
    overtemperature_fault: bool,
    fan_locked_fault: bool,
    battery_low_fault: bool,
    overload_fault: bool,
    output_short_fault: bool,
) -> lib_parse::canonical::CanonicalInverterFault {
    lib_parse::canonical::CanonicalInverterFault {
        line_fail,
        overtemperature_fault,
        fan_locked_fault,
        battery_low_fault,
        overload_fault,
        output_short_fault,
        raw_bits: 0,
    }
}

fn wire_ac_inv(
    reading: &lib_parse::canonical::CanonicalInverterReading,
    device_id: &Identity,
    t: ClockMonotonic,
) -> Value {
    let mut buf = [0u8; 8192];
    let n = lib_mapping::render::render_ac_measurement_inverter(
        device_id.clone(),
        reading,
        t,
        &mut buf,
    )
    .expect("render_ac_measurement_inverter");
    serde_json::from_slice(&buf[..n]).expect("wire ac_measurement_inverter is valid JSON")
}

fn serde_ac_inv(
    reading: &lib_parse::canonical::CanonicalInverterReading,
    device_id: &Identity,
    t: ClockMonotonic,
) -> Value {
    let payload = lib_mapping::transform_inverter::to_ac_measurement(reading, device_id.clone(), t);
    let mut buf = [0u8; 8192];
    let n = to_json_slice(&payload, &mut buf).expect("to_json_slice(AcMeasurementUpdate)");
    serde_json::from_slice(&buf[..n]).expect("serde ac_measurement_inverter is valid JSON")
}

fn wire_dc_inv(
    reading: &lib_parse::canonical::CanonicalInverterReading,
    device_id: &Identity,
    t: ClockMonotonic,
) -> Value {
    let mut buf = [0u8; 8192];
    let n = lib_mapping::render::render_dc_measurement_inverter(
        device_id.clone(),
        reading,
        t,
        &mut buf,
    )
    .expect("render_dc_measurement_inverter");
    serde_json::from_slice(&buf[..n]).expect("wire dc_measurement_inverter is valid JSON")
}

fn serde_dc_inv(
    reading: &lib_parse::canonical::CanonicalInverterReading,
    device_id: &Identity,
    t: ClockMonotonic,
) -> Value {
    let payload = lib_mapping::transform_inverter::to_dc_measurement(reading, device_id.clone(), t);
    let mut buf = [0u8; 8192];
    let n = to_json_slice(&payload, &mut buf).expect("to_json_slice(DcMeasurementUpdate)");
    serde_json::from_slice(&buf[..n]).expect("serde dc_measurement_inverter is valid JSON")
}

fn wire_hw_inv(
    reading: &lib_parse::canonical::CanonicalInverterReading,
    device_id: &Identity,
    t: ClockMonotonic,
) -> Value {
    let mut buf = [0u8; 8192];
    let n = lib_mapping::render::render_power_hardware_inverter(
        device_id.clone(),
        reading,
        t,
        &mut buf,
    )
    .expect("render_power_hardware_inverter");
    serde_json::from_slice(&buf[..n]).expect("wire power_hardware_inverter is valid JSON")
}

fn serde_hw_inv(
    reading: &lib_parse::canonical::CanonicalInverterReading,
    device_id: &Identity,
    t: ClockMonotonic,
) -> Value {
    let payload = lib_mapping::transform_inverter::to_power_hardware(reading, device_id.clone(), t);
    let mut buf = [0u8; 8192];
    let n = to_json_slice(&payload, &mut buf).expect("to_json_slice(PowerHardwareUpdate)");
    serde_json::from_slice(&buf[..n]).expect("serde power_hardware_inverter is valid JSON")
}

fn wire_ads_inv(
    fault: &lib_parse::canonical::CanonicalInverterFault,
    mode: lib_parse::canonical::InverterMode,
    device_id: &Identity,
    t: ClockMonotonic,
) -> Value {
    let mut buf = [0u8; 8192];
    let n = lib_mapping::render::render_active_diagnostic_inverter(
        fault,
        mode,
        device_id.clone(),
        t,
        &mut buf,
    )
    .expect("render_active_diagnostic_inverter");
    serde_json::from_slice(&buf[..n]).expect("wire active_diagnostic_inverter is valid JSON")
}

fn serde_ads_inv(
    fault: &lib_parse::canonical::CanonicalInverterFault,
    mode: lib_parse::canonical::InverterMode,
    device_id: &Identity,
    t: ClockMonotonic,
) -> Value {
    let payload =
        lib_mapping::transform_inverter::to_active_diagnostics(fault, mode, device_id.clone(), t);
    let mut buf = [0u8; 8192];
    let n = to_json_slice(&payload, &mut buf).expect("to_json_slice(ActiveDiagnosticState)");
    serde_json::from_slice(&buf[..n]).expect("serde active_diagnostic_inverter is valid JSON")
}

fn wire_storage(reading: &CanonicalReading, device_id: &Identity, _t: ClockMonotonic) -> Value {
    let mut buf = [0u8; 8192];
    let n = lib_mapping::render::render_storage_update(device_id.clone(), reading, &mut buf)
        .expect("render_storage_update");
    serde_json::from_slice(&buf[..n]).expect("wire storage_update is valid JSON")
}

fn serde_storage(reading: &CanonicalReading, device_id: &Identity, t: ClockMonotonic) -> Value {
    let payload = reading.to_storage_update(device_id.clone(), t);
    let mut buf = [0u8; 8192];
    let n = to_json_slice(&payload, &mut buf).expect("to_json_slice(StorageUpdate)");
    serde_json::from_slice(&buf[..n]).expect("serde storage_update is valid JSON")
}

/// `AcMeasurementUpdate` (inverter sourced) — wire vs serde across representative fixtures: normal
/// operation, divide-by-zero (output voltage=0).
#[test]
fn ac_measurement_inverter() {
    let d = dev("METEOR-001-inv");
    let t = now(12345, 678_000_000);
    let mut report = Vec::new();

    // Normal operation
    let r = inverter_reading(
        2304, 500, 2300, 500, 900, 850, 3800, 50, 1900, 5120, 8, 0, 27,
    );
    fold(
        &mut report,
        "ac_measurement_inv/normal",
        &serde_ac_inv(&r, &d, t),
        &wire_ac_inv(&r, &d, t),
    );

    // Divide-by-zero: AC output voltage = 0 (amperage should be NaN)
    let r = inverter_reading(2304, 500, 0, 500, 900, 850, 3800, 50, 1900, 5120, 8, 0, 27);
    fold(
        &mut report,
        "ac_measurement_inv/voltage_zero",
        &serde_ac_inv(&r, &d, t),
        &wire_ac_inv(&r, &d, t),
    );

    assert!(
        report.is_empty(),
        "ac_measurement_inverter wire/serde shape divergence:\n{}",
        report.join("\n")
    );
}

/// `DcMeasurementUpdate` (inverter sourced) — wire vs serde across representative PV/battery current combos.
#[test]
fn dc_measurement_inverter() {
    let d = dev("METEOR-001-inv");
    let t = now(12345, 678_000_000);
    let mut report = Vec::new();

    for &(pv_curr_da, batt_charge, batt_discharge) in &[
        (50u16, 8u16, 0u16), // normal: PV charging, battery charging
        (0, 0, 10),          // normal: PV off, battery discharging
        (100, 0, 0),         // PV only
    ] {
        let label = format!(
            "dc_measurement_inv/pv={pv_curr_da}/batt_ch={batt_charge}/batt_dis={batt_discharge}"
        );
        let r = inverter_reading(
            2304,
            500,
            2300,
            500,
            900,
            850,
            3800,
            pv_curr_da,
            1900,
            5120,
            batt_charge,
            batt_discharge,
            27,
        );
        fold(
            &mut report,
            &label,
            &serde_dc_inv(&r, &d, t),
            &wire_dc_inv(&r, &d, t),
        );
    }

    assert!(
        report.is_empty(),
        "dc_measurement_inverter wire/serde shape divergence:\n{}",
        report.join("\n")
    );
}

/// `PowerHardwareUpdate` (inverter sourced) — wire vs serde with different temperatures.
#[test]
fn power_hardware_inverter() {
    let d = dev("METEOR-001-inv");
    let t = now(12345, 678_000_000);
    let mut report = Vec::new();

    for &temp in &[0i16, 27, 50, -10] {
        let label = format!("power_hardware_inv/temp={temp}");
        let r = inverter_reading(
            2304, 500, 2300, 500, 900, 850, 3800, 50, 1900, 5120, 8, 0, temp,
        );
        fold(
            &mut report,
            &label,
            &serde_hw_inv(&r, &d, t),
            &wire_hw_inv(&r, &d, t),
        );
    }

    assert!(
        report.is_empty(),
        "power_hardware_inverter wire/serde shape divergence:\n{}",
        report.join("\n")
    );
}

/// `ActiveDiagnosticState` (inverter sourced) — wire vs serde across fault combinations and modes,
/// including synthesis of SPN 130 DTC for Fault mode.
#[test]
fn active_diagnostic_inverter() {
    let d = dev("METEOR-001-inv");
    let t = now(12345, 678_000_000);
    let mut report = Vec::new();

    // Zero faults, normal mode
    let f = inverter_fault(false, false, false, false, false, false);
    fold(
        &mut report,
        "active_diagnostic_inv/zero_faults_normal",
        &serde_ads_inv(&f, lib_parse::canonical::InverterMode::Line, &d, t),
        &wire_ads_inv(&f, lib_parse::canonical::InverterMode::Line, &d, t),
    );

    // Zero faults, Fault mode (should synthesize SPN 130)
    fold(
        &mut report,
        "active_diagnostic_inv/zero_faults_fault_mode",
        &serde_ads_inv(&f, lib_parse::canonical::InverterMode::Fault, &d, t),
        &wire_ads_inv(&f, lib_parse::canonical::InverterMode::Fault, &d, t),
    );

    // Several faults, normal mode
    let f = inverter_fault(true, true, false, false, false, false);
    fold(
        &mut report,
        "active_diagnostic_inv/several_faults_normal",
        &serde_ads_inv(&f, lib_parse::canonical::InverterMode::Line, &d, t),
        &wire_ads_inv(&f, lib_parse::canonical::InverterMode::Line, &d, t),
    );

    // Several faults, Fault mode (should synthesize SPN 130 in addition)
    fold(
        &mut report,
        "active_diagnostic_inv/several_faults_fault_mode",
        &serde_ads_inv(&f, lib_parse::canonical::InverterMode::Fault, &d, t),
        &wire_ads_inv(&f, lib_parse::canonical::InverterMode::Fault, &d, t),
    );

    assert!(
        report.is_empty(),
        "active_diagnostic_inverter wire/serde shape divergence:\n{}",
        report.join("\n")
    );
}

/// `StorageUpdate` — wire vs serde across representative battery readings (SOC, temperature variations).
#[test]
fn storage_update() {
    let d = dev("METEOR-001");
    let t = now(12345, 678_000_000);
    let mut report = Vec::new();

    // All cells and temps present
    let r = reading(14, 6, -164, 5452, 775, 2);
    fold(
        &mut report,
        "storage_update/full",
        &serde_storage(&r, &d, t),
        &wire_storage(&r, &d, t),
    );

    // Minimal: 0 cells, 0 temps
    let r = reading(0, 0, 0, 5452, 500, 1);
    fold(
        &mut report,
        "storage_update/empty",
        &serde_storage(&r, &d, t),
        &wire_storage(&r, &d, t),
    );

    // Live frame
    let r = live_reading();
    fold(
        &mut report,
        "storage_update/LIVE_FRAME",
        &serde_storage(&r, &d, t),
        &wire_storage(&r, &d, t),
    );

    assert!(
        report.is_empty(),
        "storage_update wire/serde shape divergence:\n{}",
        report.join("\n")
    );
}

// ---- IDENTITY TOPICS: Tier 2 — exact bytes ----

/// `Heartbeat` exact-byte oracle: render_value must produce byte-identical JSON to to_json_slice.
#[test]
fn heartbeat_exact_bytes() {
    let d = dev("METEOR-001");
    let hb = Heartbeat {
        device_id: d.clone(),
        sequence_number: 42,
    };
    let mut buf_render = [0u8; 8192];
    let n_render = render_value(&hb, &mut buf_render).expect("render_value(Heartbeat)");
    let mut buf_serde = [0u8; 8192];
    let n_serde = to_json_slice(&hb, &mut buf_serde).expect("to_json_slice(Heartbeat)");
    exact_bytes(&buf_serde[..n_serde], &buf_render[..n_render]);
}

/// `Reply` exact-byte oracle with multiple status variants.
#[test]
fn reply_exact_bytes() {
    let d = dev("METEOR-001");
    let mut buf_render = [0u8; 8192];
    let mut buf_serde = [0u8; 8192];

    // NotImplemented status
    let reply = Reply {
        requesting_device_id: d.clone(),
        target_device_id: d.clone(),
        config: ConfigId::Unknown,
        port_number: 65535,
        request_sequence_id: 1,
        status: ReplyStatus {
            code: ReplyCode::NotImplemented,
            reason: heapless::String::try_from("not implemented").unwrap(),
        },
    };
    let n_render =
        render_value(&reply, &mut buf_render).expect("render_value(Reply/NotImplemented)");
    let n_serde = to_json_slice(&reply, &mut buf_serde).expect("to_json_slice(Reply)");
    exact_bytes(&buf_serde[..n_serde], &buf_render[..n_render]);

    // BadRequest status
    let reply_bad = Reply {
        requesting_device_id: d.clone(),
        target_device_id: d.clone(),
        config: ConfigId::Unknown,
        port_number: 65535,
        request_sequence_id: 2,
        status: ReplyStatus {
            code: ReplyCode::BadRequest,
            reason: heapless::String::try_from("malformed").unwrap(),
        },
    };
    buf_render = [0u8; 8192];
    buf_serde = [0u8; 8192];
    let n_render =
        render_value(&reply_bad, &mut buf_render).expect("render_value(Reply/BadRequest)");
    let n_serde =
        to_json_slice(&reply_bad, &mut buf_serde).expect("to_json_slice(Reply/BadRequest)");
    exact_bytes(&buf_serde[..n_serde], &buf_render[..n_render]);
}

/// `EnergyStartStopState` exact-byte oracle.
#[test]
fn energy_start_stop_state_exact_bytes() {
    let d: Identity = heapless::String::try_from("METEOR-001").unwrap();
    let state = EnergyStartStopState {
        device_id: d.clone(),
        present_level: EnergyStartStopLevel::Operational,
        future_level: EnergyStartStopLevel::Operational,
        request_lock: false,
        elapsed_time: 0.0,
        remaining_time: f32::NAN,
        present_level_reason: PowerSwitchReason::Startup,
        present_level_actor: None,
        future_level_reason: PowerSwitchTransitionCode::None,
    };
    let mut buf_render = [0u8; 8192];
    let n_render =
        render_value(&state, &mut buf_render).expect("render_value(EnergyStartStopState)");
    let mut buf_serde = [0u8; 8192];
    let n_serde =
        to_json_slice(&state, &mut buf_serde).expect("to_json_slice(EnergyStartStopState)");
    exact_bytes(&buf_serde[..n_serde], &buf_render[..n_render]);
}

/// `PowerPortState` exact-byte oracle.
#[test]
fn power_port_state_exact_bytes() {
    let d: Identity = heapless::String::try_from("METEOR-001").unwrap();
    let mut ports = heapless::Vec::new();
    ports
        .push(PowerPortStatus {
            port_number: 0,
            cable_status: lib_mapping::schema::CableSenseStatus::Unknown,
            cable_id: None,
            connection: None,
            switch_status: None,
        })
        .unwrap();
    let state = PowerPortState {
        device_id: d.clone(),
        power_ports: ports,
    };
    let mut buf_render = [0u8; 8192];
    let n_render = render_value(&state, &mut buf_render).expect("render_value(PowerPortState)");
    let mut buf_serde = [0u8; 8192];
    let n_serde = to_json_slice(&state, &mut buf_serde).expect("to_json_slice(PowerPortState)");
    exact_bytes(&buf_serde[..n_serde], &buf_render[..n_render]);
}

/// `DcLoadSharingState` exact-byte oracle.
#[test]
fn dc_load_sharing_state_exact_bytes() {
    let d: Identity = heapless::String::try_from("METEOR-001").unwrap();
    let state = DcLoadSharingState {
        device_id: d.clone(),
        config: ConfigId::Unknown,
        parameters: heapless::Vec::new(),
    };
    let mut buf_render = [0u8; 8192];
    let n_render = render_value(&state, &mut buf_render).expect("render_value(DcLoadSharingState)");
    let mut buf_serde = [0u8; 8192];
    let n_serde = to_json_slice(&state, &mut buf_serde).expect("to_json_slice(DcLoadSharingState)");
    exact_bytes(&buf_serde[..n_serde], &buf_render[..n_render]);
}

/// `ActiveMicrogridControllerState` exact-byte oracle.
#[test]
fn active_microgrid_controller_state_exact_bytes() {
    let d: Identity = heapless::String::try_from("METEOR-001").unwrap();
    let master: Identity = heapless::String::try_from("MC-0001").unwrap();
    let state = ActiveMicrogridControllerState {
        device_id: d.clone(),
        master_id: Some(master),
    };
    let mut buf_render = [0u8; 8192];
    let n_render = render_value(&state, &mut buf_render)
        .expect("render_value(ActiveMicrogridControllerState)");
    let mut buf_serde = [0u8; 8192];
    let n_serde = to_json_slice(&state, &mut buf_serde)
        .expect("to_json_slice(ActiveMicrogridControllerState)");
    exact_bytes(&buf_serde[..n_serde], &buf_render[..n_render]);
}

/// `DeviceInfo` exact-byte oracle — minimal variant, storage role.
#[test]
fn device_info_exact_bytes() {
    let d: Identity = heapless::String::try_from("METEOR-001").unwrap();

    // Minimal DeviceInfo
    let info = DeviceInfo {
        device_id: d.clone(),
        role: DeviceRole::Storage,
        product: lib_mapping::schema::ProductInfo {
            nsn: heapless::String::new(),
            gtin: heapless::String::new(),
            manufacturer_name: heapless::String::try_from("HVD").unwrap(),
            model_name: heapless::String::try_from("METEOR").unwrap(),
            model_number: heapless::String::new(),
            serial_number: heapless::String::try_from("SN-0001").unwrap(),
            software_version: heapless::String::try_from("0.3.0").unwrap(),
            platform_id: None,
        },
        topics: lib_mapping::schema::TopicInfo {
            data_model_version: heapless::String::try_from("2.0.2").unwrap(),
            published_conditional_topics: heapless::Vec::new(),
            published_optional_topics: heapless::Vec::new(),
            supported_request_topics: heapless::Vec::new(),
            extension_topics: None,
        },
        control_hardware: None,
        power_hardware: None,
        power_device: None,
        control_parameters: None,
        metric_parameters: None,
        control_service: None,
    };
    let mut buf_render = [0u8; 8192];
    let n_render = render_value(&info, &mut buf_render).expect("render_value(DeviceInfo/minimal)");
    let mut buf_serde = [0u8; 8192];
    let n_serde = to_json_slice(&info, &mut buf_serde).expect("to_json_slice(DeviceInfo)");
    exact_bytes(&buf_serde[..n_serde], &buf_render[..n_render]);
}
