//! PROOF — `WriteSink`/`CountingSink`'s `JsonSink::f32`/`f64` methods produce byte-identical output
//! to `serde_json_core::to_slice`'s float serialization (the on-device JSON encoder), across a set
//! of representative finite/subnormal/large values plus non-finite values (`NaN`/`±Infinity` ->
//! `null`, matching `serde_json_core` 0.6.0's `Serializer::serialize_f32`/`serialize_f64`,
//! `serde-json-core-0.6.0/src/ser/mod.rs:304-318`). `CountingSink`'s counted length is also checked
//! against `WriteSink`'s emitted length for every sample — the same shared-traversal invariant
//! `wire_len_matches_emit.rs` proves for the integer/fixed-point path.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_mapping::wire::{CountingSink, JsonSink, WriteSink};

fn oracle_f32(v: f32) -> String {
    let mut buf = [0u8; 64];
    let len = serde_json_core::to_slice(&v, &mut buf).expect("encode");
    core::str::from_utf8(&buf[..len]).expect("utf8").to_string()
}

fn oracle_f64(v: f64) -> String {
    let mut buf = [0u8; 64];
    let len = serde_json_core::to_slice(&v, &mut buf).expect("encode");
    core::str::from_utf8(&buf[..len]).expect("utf8").to_string()
}

fn check_f32(v: f32) {
    let mut counting = CountingSink::new();
    counting.f32(v);
    let mut buf = [0u8; 64];
    let mut writer = WriteSink::new(&mut buf);
    writer.f32(v);
    assert!(!writer.overflowed(), "overflow for {v:?}");
    let written = writer.len();
    let emitted = core::str::from_utf8(&buf[..written]).expect("utf8");
    assert_eq!(emitted, oracle_f32(v), "byte mismatch for {v:?}");
    assert_eq!(counting.len(), written, "len mismatch for {v:?}");
}

fn check_f64(v: f64) {
    let mut counting = CountingSink::new();
    counting.f64(v);
    let mut buf = [0u8; 64];
    let mut writer = WriteSink::new(&mut buf);
    writer.f64(v);
    assert!(!writer.overflowed(), "overflow for {v:?}");
    let written = writer.len();
    let emitted = core::str::from_utf8(&buf[..written]).expect("utf8");
    assert_eq!(emitted, oracle_f64(v), "byte mismatch for {v:?}");
    assert_eq!(counting.len(), written, "len mismatch for {v:?}");
}

#[test]
fn f32_samples_byte_identical_to_serde_json_core() {
    for v in [
        0.0f32,
        -0.0f32,
        1.0f32,
        23.5f32,
        42.0f32,
        1e20f32,
        1.5e-45f32,
        f32::MIN_POSITIVE,
        -273.15f32,
    ] {
        check_f32(v);
    }
}

/// `NaN`/`±Infinity` -> `null` (never a `ryu`-formatted literal `ryu` cannot represent).
#[test]
fn f32_non_finite_is_null() {
    for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        check_f32(v);
        assert_eq!(oracle_f32(v), "null");
    }
}

#[test]
fn f64_samples_byte_identical_to_serde_json_core() {
    for v in [
        0.0f64,
        -0.0f64,
        1.0f64,
        23.5f64,
        42.0f64,
        1e20f64,
        1.5e-45f64,
        f64::MIN_POSITIVE,
        -273.15f64,
    ] {
        check_f64(v);
    }
}

/// `NaN`/`±Infinity` -> `null`, `f64` counterpart of `f32_non_finite_is_null`.
#[test]
fn f64_non_finite_is_null() {
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        check_f64(v);
        assert_eq!(oracle_f64(v), "null");
    }
}
