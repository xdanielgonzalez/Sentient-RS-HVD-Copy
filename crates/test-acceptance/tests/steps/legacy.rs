//! Steps for the legacy AWS Greengrass backwards-compat feature area (Scope D, closes #52-#54,
//! reconciled against the real baseline — gap-closure #269) — see
//! `docs/gap-analysis-meteor-parity.md`'s "Greengrass local IPC publish" finding.
//!
//! GREP-GATE — no re-implemented mapping logic lives here. Every scenario below calls straight
//! into the real, reused `lib_mapping::legacy::battery_payload` builder and asserts on the parsed
//! `serde_json::Value` it returns. This file is Gherkin<->Rust plumbing only: staging a
//! `CanonicalReading`/`CanonicalFault`/`LegacyLocationFix` fixture, calling the one real conversion
//! under test, and asserting on the resulting JSON shape.
//!
//! The broker-publish half of Scope D (`bin_rpi_platform::GreengrassTap` offering a `TapEvent` onto
//! a real embedded broker, and the mosquitto-bridge render path) is exercised end to end in
//! `test-e2e/tests/legacy_greengrass_e2e.rs` and unit-tested directly in
//! `bin-rpi/src/mosquitto_bridge.rs` — this in-process suite covers only the flat-JSON SHAPE,
//! mirroring how `steps/mqtt.rs` keeps broker delivery out of the cucumber suite entirely.
//!
//! **BASELINE-VERIFIED, not provisional** (docs/tms-compliance.md §7) — every battery scenario in
//! `legacy_greengrass.feature` DROPS the `@provisional` tag it carried before this reconciliation:
//! `/mnt/d/hvd/meteor-adapter`'s real `example/aws/src/greengrass/telemetry/battery/
//! {battery_reader.py,config.yaml}` and `common/device_telemetry_base.py` are checked in and were
//! read directly (see `lib_mapping::legacy`'s own module doc comment) — the field set, scale
//! factors, and omissions this suite pins are a confirmed spec, not a best-effort guess.
//!
//! Test code only; freely uses unwrap/expect/panic (the workspace lint gate denies these in
//! production logic, not tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cucumber::{given, then, when};
use serde_json::Value;

use lib_mapping::legacy::{battery_payload, LegacyLocationFix};
use lib_parse::canonical::CanonicalReading;

use crate::SpecWorld;

/// Scenario scratch for the legacy Greengrass feature area. cucumber resets `SpecWorld` per
/// scenario, so every field starts `None` and is populated only by the steps a given scenario
/// actually runs.
///
/// No `CanonicalFault` field here: the fault-merge scenarios reuse `steps/mapping.rs`'s already
/// -registered `"a canonical fault with flag {word} set"` / `"a canonical fault with no flags
/// set"` Given steps (which populate the shared top-level `SpecWorld::fault`), rather than
/// re-registering the same step text a second time — cucumber matches step text globally, so a
/// second registration of the identical pattern would be ambiguous.
#[derive(Debug, Default)]
pub struct LegacyState {
    reading: Option<CanonicalReading>,
    /// Staged by `given_location_tag`, consumed by `when_render_reading_with_location`.
    location: Option<LegacyLocationFix>,
    /// The merged battery payload most recently rendered, parsed back into a `Value` for
    /// key-by-key assertions (the payload builder itself only returns raw bytes).
    payload: Option<Value>,
}

/// The unit-level device identity `battery_payload` stamps into every payload's own `device_id`
/// field — the caller's configured identity, NOT the per-frame Seplos DIP address (see
/// `lib_mapping::legacy::battery_payload`'s doc comment). Matches `crate::dev()`'s spelling so
/// every feature area in this suite pins the same device identity.
const LEGACY_DEVICE_ID: &str = "METEOR-001";

/// Fixed wall-clock ms used by every mapping scenario (mirrors `crate::now()`'s role for the other
/// feature areas — `battery_payload` takes a plain `u64` ms, not a `ClockMonotonic`, so it does
/// not reuse `crate::now()` directly).
const LEGACY_WALL_MS: u64 = 1_700_000_000_000;

/// The same golden 2-cell/1-temperature-sensor reading `test-e2e/tests/legacy_greengrass_e2e.rs`
/// and `test-e2e/tests/broker_roundtrip.rs` (T8) use, so every legacy-JSON value asserted in this
/// suite is checkable against the same fixture the e2e/broker tests pin numerically. Only the
/// Seplos DIP address varies per scenario (parameterizing the payload's `dip_address` field).
fn golden_reading(device_address: u8) -> CanonicalReading {
    let mut cells = heapless::Vec::new();
    cells.push(3894).expect("cell 0 fits MAX_CELLS");
    cells.push(3901).expect("cell 1 fits MAX_CELLS");
    let mut temps = heapless::Vec::new();
    temps.push(2991u16).expect("temp 0 fits MAX_TEMPS"); // (2991-2731)/10 = 26.0 C
    CanonicalReading {
        device_address,
        cell_voltages_mv: cells,
        temperatures_raw: temps,
        current_ca: -144,
        total_voltage_cv: 5451,
        remaining_capacity_cah: 13953,
        typical_capacity_cah: 18000,
        rated_capacity_cah: 18000,
        soc_dpct: 775,
        soh_dpct: 1000,
        cycles: 2,
        port_voltage_cv: 5453,
    }
}

/// A bare location-source word (e.g. `gps_nmea`) -> the matching `'static` string constant
/// `LegacyLocationFix::source` needs (see that field's own doc comment for why it's `&'static
/// str`, not `String`) — every value this suite's feature file uses is one of the four baseline
/// provider tags, so an exhaustive match is all a Gherkin-captured `String` ever needs.
fn static_source(word: &str) -> &'static str {
    match word {
        "manual" => "manual",
        "gps_nmea" => "gps_nmea",
        "modem_manager" => "modem_manager",
        "ip_geo" => "ip_geo",
        other => panic!("unknown location source in feature text: {other}"),
    }
}

// ---- Given: fixtures -----------------------------------------------------------------------

#[given(expr = "the golden canonical reading at Seplos address {int}")]
fn given_golden_reading(world: &mut SpecWorld, address: i64) {
    world.legacy.reading = Some(golden_reading(address as u8));
}

#[given(expr = "a legacy location tag at lat {float} lon {float} from {word}")]
fn given_location_tag(world: &mut SpecWorld, lat: f64, lon: f64, source: String) {
    world.legacy.location = Some(LegacyLocationFix {
        latitude: lat,
        longitude: lon,
        source: static_source(&source),
        ts_ms: LEGACY_WALL_MS,
    });
}

// ---- When: the real lib_mapping::legacy::battery_payload builder ---------------------------

#[when("the reading is rendered as a merged battery payload")]
fn when_render_reading(world: &mut SpecWorld) {
    // `CanonicalReading` is not `Copy` (it carries `heapless::Vec` fields) — clone it out of the
    // scenario scratch first, same convention `steps/resilience.rs`'s `when_map_hw_res` uses for
    // `SpecWorld::reading`.
    let reading = world.legacy.reading.clone().expect("golden reading staged");
    render(world, &reading, None, None);
}

#[when("the reading is rendered as a merged battery payload with its location tag")]
fn when_render_reading_with_location(world: &mut SpecWorld) {
    let reading = world.legacy.reading.clone().expect("golden reading staged");
    let loc = world.legacy.location.expect("location tag staged");
    render(world, &reading, None, Some(&loc));
}

#[when("the reading and fault are rendered as a merged battery payload")]
fn when_render_reading_and_fault(world: &mut SpecWorld) {
    let reading = world.legacy.reading.clone().expect("golden reading staged");
    // Reuses `steps/mapping.rs`'s shared top-level `SpecWorld::fault` (populated by its
    // already-registered "a canonical fault with flag {word} set" / "...with no flags set" Given
    // steps — see `LegacyState`'s doc comment for why this file does not re-stage its own).
    let fault = world.fault.expect("fault staged");
    render(world, &reading, Some(&fault), None);
}

fn render(
    world: &mut SpecWorld,
    reading: &CanonicalReading,
    fault: Option<&lib_parse::canonical::CanonicalFault>,
    loc: Option<&LegacyLocationFix>,
) {
    let bytes = battery_payload(
        LEGACY_DEVICE_ID,
        reading,
        fault,
        loc,
        LEGACY_WALL_MS,
        reading.device_address,
    )
    .expect("serialize merged battery payload");
    world.legacy.payload = Some(serde_json::from_slice(&bytes).expect("valid json"));
}

// ---- Then: assertions on the merged flat JSON shape -----------------------------------------

#[then(expr = "the payload device id is {string}")]
fn then_device_id(world: &mut SpecWorld, expected: String) {
    assert_eq!(payload(world)["device_id"], expected);
}

#[then(expr = "the payload device type is {string}")]
fn then_device_type(world: &mut SpecWorld, expected: String) {
    assert_eq!(payload(world)["device_type"], expected);
}

#[then(expr = "the payload dip address is {int}")]
fn then_dip_address(world: &mut SpecWorld, expected: i64) {
    assert_eq!(payload(world)["dip_address"], expected);
}

#[then(expr = "the payload timestamp ends with {string}")]
fn then_timestamp_ends_with(world: &mut SpecWorld, suffix: String) {
    let ts = payload(world)["timestamp"]
        .as_str()
        .expect("timestamp is a string")
        .to_string();
    assert!(ts.ends_with(&suffix), "timestamp {ts} !~ suffix {suffix}");
}

#[then(expr = "the payload current is approximately {float} A")]
fn then_current(world: &mut SpecWorld, expected: f64) {
    let v = payload(world)["current"].as_f64().expect("current number");
    assert!((v - expected).abs() < 0.01, "current {v} !~ {expected}");
}

#[then(expr = "the payload residual capacity is approximately {float} Ah")]
fn then_residual_capacity(world: &mut SpecWorld, expected: f64) {
    let v = payload(world)["residual_capacity"]
        .as_f64()
        .expect("residual_capacity number");
    assert!((v - expected).abs() < 0.01);
}

#[then(expr = "the payload battery capacity is approximately {float} Ah")]
fn then_battery_capacity(world: &mut SpecWorld, expected: f64) {
    let v = payload(world)["battery_capacity"]
        .as_f64()
        .expect("battery_capacity number");
    assert!((v - expected).abs() < 0.01);
}

#[then(expr = "the payload state of charge is approximately {float} percent")]
fn then_soc(world: &mut SpecWorld, expected: f64) {
    let v = payload(world)["state_of_charge"]
        .as_f64()
        .expect("state_of_charge number");
    assert!((v - expected).abs() < 0.01);
}

#[then(expr = "the payload state of health is approximately {float} percent")]
fn then_soh(world: &mut SpecWorld, expected: f64) {
    let v = payload(world)["state_of_health"]
        .as_f64()
        .expect("state_of_health number");
    assert!((v - expected).abs() < 0.01);
}

#[then(expr = "the payload has {int} num_cycles")]
fn then_num_cycles(world: &mut SpecWorld, expected: i64) {
    assert_eq!(payload(world)["num_cycles"], expected);
}

#[then(expr = "the payload port voltage is approximately {float} V")]
fn then_port_voltage(world: &mut SpecWorld, expected: f64) {
    let v = payload(world)["port_voltage"]
        .as_f64()
        .expect("port_voltage number");
    assert!((v - expected).abs() < 0.01);
}

#[then(expr = "the payload has no {string} key")]
fn then_no_key(world: &mut SpecWorld, key: String) {
    assert!(
        payload(world).get(&key).is_none(),
        "expected no {key:?} key in the merged payload"
    );
}

#[then(expr = "the payload has {int} num_cells")]
fn then_num_cells(world: &mut SpecWorld, expected: i64) {
    assert_eq!(payload(world)["num_cells"], expected);
}

#[then(expr = "payload cell_{int}_voltage is {int} mV")]
fn then_cell_voltage(world: &mut SpecWorld, i: i64, expected: i64) {
    let key = format!("cell_{i}_voltage");
    assert_eq!(payload(world)[key.as_str()], expected);
}

#[then(expr = "payload temperature_{int} is approximately {float} C")]
fn then_temperature(world: &mut SpecWorld, i: i64, expected: f64) {
    let key = format!("temperature_{i}");
    let v = payload(world)[key.as_str()]
        .as_f64()
        .expect("temperature number");
    assert!((v - expected).abs() < 0.01);
}

#[then(expr = "the payload location source is {string}")]
fn then_location_source(world: &mut SpecWorld, expected: String) {
    assert_eq!(payload(world)["location_source"], expected);
}

#[then(expr = "the payload latitude is approximately {float}")]
fn then_latitude_approx(world: &mut SpecWorld, expected: f64) {
    let v = payload(world)["latitude"]
        .as_f64()
        .expect("latitude number");
    assert!((v - expected).abs() < 1e-6);
}

#[then(expr = "the payload fault flag {word} is {int}")]
fn then_fault_flag(world: &mut SpecWorld, flag: String, expected: i64) {
    assert_eq!(payload(world)[flag.as_str()], expected);
}

#[then("the payload is valid JSON with a string device_id")]
fn then_valid_json(world: &mut SpecWorld) {
    assert!(payload(world)["device_id"].is_string());
}

// ---- shared accessors ------------------------------------------------------------------------

fn payload(world: &SpecWorld) -> &Value {
    world.legacy.payload.as_ref().expect("payload rendered")
}
