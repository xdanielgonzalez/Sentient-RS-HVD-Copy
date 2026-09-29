//! `render_tms`/`render_fault` (the serialize-on-demand public seam, `src/render.rs`) — value
//! correctness against the real Seplos `LIVE_FRAME` fixture, exercised through the exact public API
//! `bin_zephyr_platform::render_pending_topic` and `lib_mapping::sink::publish_tms` call.
//!
//! `render_tms`/`render_fault` are now backed by [`lib_mapping::wire`]'s flat serializer, not the
//! old serde/`ryu` pipeline (`crate::codec::to_json_slice` + `ToTms`/`ToDiagnostics` structs) — so
//! this no longer asserts byte-identical output against that old pipeline (fixed-precision decimals
//! differ from `ryu`'s shortest-round-trip floats, e.g. `54.5` may render `54.50`). Instead it
//! parses the emitted JSON and checks values within the TMS accuracy tolerance (+/-0.005,
//! `docs/tms-compliance.md`) — the same golden values `docs/wire-serialization.md`'s plan calls out
//! (voltage 54.52, amperage +1.64 sign-inverted, `run_time`/`power_rate_of_change` -> JSON `null`).
//! Field-level value math is exercised in depth by `wire_value_correctness.rs`/
//! `wire_len_matches_emit.rs` against `crate::wire` directly; this file's job is only to confirm
//! `render_tms`/`render_fault`'s public seam still produces the right thing end to end.

// Test code may unwrap/panic; the workspace denies these in logic only.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use heapless::String;
use lib_mapping::render::{render_fault, render_tms, TmsTopic};
use lib_mapping::schema::ClockMonotonic;
use lib_parse::canonical::CanonicalFault;
use lib_parse::seplos::Seplos;
use lib_parse::vendor::BmsVendorProtocol;
use serde_json::Value;

const LIVE_FRAME: &[u8] = b"~20004600A08E00000E0F360F360F360F360F350F360F360F360F360F370F370F360F360F35060AF10AF90AF90AF30B370B0CFF5C154C36820A465003074650000203E8154CFFF800000079004DDEC8\r";
const TOL: f64 = 0.005;

fn assert_close(actual: f64, expected: f64, label: &str) {
    assert!(
        (actual - expected).abs() <= TOL,
        "{label}: got {actual}, expected {expected} +/- {TOL}"
    );
}

fn dev() -> String<32> {
    let mut s = String::new();
    s.push_str("METEOR-001").ok();
    s
}

fn now_zero() -> ClockMonotonic {
    ClockMonotonic {
        seconds: 0,
        nanoseconds: 0,
    }
}

/// `render_tms(DcMeasurementUpdate, ...)` produces valid JSON with the expected golden values.
#[test]
fn render_dc_measurement_matches_goldens() {
    let reading = Seplos::new()
        .parse_reading(LIVE_FRAME, 0)
        .expect("decode LIVE_FRAME");

    let mut got = [0u8; 4096];
    let got_n = render_tms(
        TmsTopic::DcMeasurementUpdate,
        dev(),
        &reading,
        now_zero(),
        &mut got,
    )
    .expect("render dc");

    let s = core::str::from_utf8(&got[..got_n]).expect("utf8");
    let v: Value = serde_json::from_str(s).expect("valid JSON");

    assert_eq!(v["device_id"], "METEOR-001");
    let line = &v["external_measurement"][0]["line"][0];
    assert_close(line["voltage"].as_f64().unwrap(), 54.52, "voltage");
    assert_close(line["amperage"].as_f64().unwrap(), 1.64, "amperage");
    assert!(v["external_measurement"][0]["power_rate_of_change"].is_null());
}

/// `render_tms(PowerHardwareUpdate, ...)` produces valid JSON with the expected golden values.
#[test]
fn render_power_hardware_matches_goldens() {
    let reading = Seplos::new()
        .parse_reading(LIVE_FRAME, 0)
        .expect("decode LIVE_FRAME");

    let mut got = [0u8; 4096];
    let got_n = render_tms(
        TmsTopic::PowerHardwareUpdate,
        dev(),
        &reading,
        now_zero(),
        &mut got,
    )
    .expect("render hw");

    let s = core::str::from_utf8(&got[..got_n]).expect("utf8");
    let v: Value = serde_json::from_str(s).expect("valid JSON");

    assert_eq!(v["device_id"], "METEOR-001");
    let cells = v["energy_storage"]["storage_cells"].as_array().unwrap();
    assert_eq!(cells.len(), 14, "14 cells");
    assert!(v["energy_storage"]["run_time"].is_null());
}

/// `render_fault` produces valid JSON with the expected shape for a 2-flag fault.
#[test]
fn render_fault_matches_goldens() {
    let mut fault = CanonicalFault {
        device_address: 0,
        ..Default::default()
    };
    fault.high_voltage_protection = true;
    fault.mosfet_fault = true;

    let mut got = [0u8; 4096];
    let got_n = render_fault(&fault, dev(), now_zero(), &mut got).expect("render fault");

    let s = core::str::from_utf8(&got[..got_n]).expect("utf8");
    let v: Value = serde_json::from_str(s).expect("valid JSON");
    assert_eq!(v["device_id"], "METEOR-001");
    let codes = v["codes"].as_array().unwrap();
    assert_eq!(codes.len(), 2, "2 DTCs");
    assert_eq!(v["overflow"], 0);
}

/// A buffer too small to hold the rendered payload reports `CodecError::BufferTooSmall`, matching
/// the old pipeline's behavior for the same failure case.
#[test]
fn render_tms_reports_buffer_too_small() {
    let reading = Seplos::new()
        .parse_reading(LIVE_FRAME, 0)
        .expect("decode LIVE_FRAME");
    let mut tiny = [0u8; 4];
    let err = render_tms(
        TmsTopic::PowerHardwareUpdate,
        dev(),
        &reading,
        now_zero(),
        &mut tiny,
    )
    .expect_err("4 bytes cannot hold a 14-cell PowerHardwareUpdate");
    assert_eq!(err, lib_mapping::codec::CodecError::BufferTooSmall);
}
