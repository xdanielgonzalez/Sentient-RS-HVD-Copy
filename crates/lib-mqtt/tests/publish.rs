//! Publish-path tests using the in-memory `MockPublisher` (no broker needed).
//!
//! Verifies topic construction (`tms/{device}/{snake_case}`) and that mapped TMS payloads serialize
//! to JSON and reach the sink. The live rumqttd broker is exercised by on-target smoke, not here.

// Test code may unwrap/panic; the workspace denies these in logic only.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use heapless::{String as HString, Vec as HVec};
use lib_mapping::schema::{topics, ClockMonotonic};
use lib_mapping::sink::publish_tms_into;
use lib_mapping::transform::ToTms;
use lib_mqtt::{publish_tms, topic_into, MockPublisher, PublishError, Publisher, DEFAULT_PREFIX};
use lib_parse::canonical::CanonicalReading;

fn topic(device: &str, tms_topic: &str) -> HString<128> {
    let mut s = HString::new();
    topic_into("tms", device, tms_topic, &mut s).expect("topic");
    s
}

fn dev_id() -> HString<32> {
    let mut s = HString::new();
    s.push_str("METEOR-001").ok();
    s
}

fn sample() -> CanonicalReading {
    let mut cells = HVec::new();
    cells.push(3894).ok();
    let temps = HVec::new();
    CanonicalReading {
        device_address: 0,
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

#[test]
fn topic_slug_is_snake_case() {
    assert_eq!(
        topic("METEOR-001", "DcMeasurementUpdate").as_str(),
        "tms/METEOR-001/dc_measurement_update"
    );
    assert_eq!(
        topic("dev", "ActiveDiagnosticState").as_str(),
        "tms/dev/active_diagnostic_state"
    );
}

#[test]
fn publishes_tms_json_to_mock() {
    let mut pubr = MockPublisher::new();
    let now = ClockMonotonic {
        seconds: 1000,
        nanoseconds: 0,
    };
    let update = sample().to_dc_measurement(dev_id(), now);

    publish_tms(
        &mut pubr,
        DEFAULT_PREFIX,
        "METEOR-001",
        topics::DC_MEASUREMENT_UPDATE,
        &update,
    )
    .expect("publish");

    assert_eq!(pubr.sent.len(), 1);
    let (topic, payload) = &pubr.sent[0];
    assert_eq!(topic, "tms/METEOR-001/dc_measurement_update");
    // Payload is valid JSON carrying the sign-inverted amperage.
    let v: serde_json::Value = serde_json::from_slice(payload).expect("valid json");
    let amps = v["external_measurement"][0]["line"][0]["amperage"]
        .as_f64()
        .unwrap();
    assert!((amps - 1.44).abs() < 0.01); // discharging -1.44 A -> TMS +1.44
}

#[test]
fn topic_into_slug_overflow_is_topic_too_long() {
    // "tms/METEOR-001/dc_measurement_update" is 36 bytes; a 35-byte buffer is one short.
    let mut out: HString<35> = HString::new();
    assert_eq!(
        topic_into("tms", "METEOR-001", "DcMeasurementUpdate", &mut out),
        Err(PublishError::TopicTooLong)
    );
}

#[test]
fn topic_into_exact_fit_boundary_succeeds() {
    // Exactly 36 bytes: "tms/METEOR-001/dc_measurement_update".
    let mut out: HString<36> = HString::new();
    topic_into("tms", "METEOR-001", "DcMeasurementUpdate", &mut out).expect("exact fit");
    assert_eq!(out.as_str(), "tms/METEOR-001/dc_measurement_update");
    assert_eq!(out.len(), 36);
}

#[test]
fn topic_into_consecutive_capitals_slug_behavior_pinned() {
    // Pin the current slugging behavior for back-to-back uppercase letters: EVERY uppercase
    // letter after position 0 gets its own leading underscore, even when the previous character
    // was also uppercase (no attempt to treat "AB" as one acronym run) — e.g. "ABTest" slugs to
    // "a_b_test", not "ab_test".
    let mut out: HString<32> = HString::new();
    topic_into("tms", "dev", "ABTest", &mut out).expect("topic");
    assert_eq!(out.as_str(), "tms/dev/a_b_test");
}

#[test]
fn publish_with_too_small_scratch_is_serialize_error_and_publishes_nothing() {
    let mut pubr = MockPublisher::new();
    let now = ClockMonotonic {
        seconds: 1000,
        nanoseconds: 0,
    };
    let update = sample().to_dc_measurement(dev_id(), now);

    // Far smaller than any real TMS JSON payload (see `lib_mapping::sink::MAX_PAYLOAD`'s doc
    // comment: a real 14-cell PowerHardwareUpdate measures ~1553 bytes; this DcMeasurementUpdate
    // is smaller still, but a single-digit-byte scratch buffer can't hold it either way).
    let mut scratch = [0u8; 4];
    let err = publish_tms_into(
        &mut pubr,
        DEFAULT_PREFIX,
        "METEOR-001",
        topics::DC_MEASUREMENT_UPDATE,
        &update,
        &mut scratch,
    )
    .expect_err("4-byte scratch must not fit a TMS JSON payload");

    assert_eq!(err, PublishError::Serialize);
    // Nothing partially published: the mock never saw a call, not even a truncated one.
    assert!(
        pubr.sent.is_empty(),
        "a failed serialize must not reach the sink at all"
    );
}

#[test]
fn mock_publisher_healthy_flag_surfaces_through_publisher_trait() {
    let mut pubr = MockPublisher::new();
    // Fresh mock is healthy by construction.
    assert!(Publisher::healthy(&pubr));

    pubr.healthy = false;
    // Flipping the plain field is visible through the trait method, not just the struct field —
    // confirms `healthy()` isn't hardcoded and actually reads `self.healthy`.
    assert!(!Publisher::healthy(&pubr));

    pubr.healthy = true;
    assert!(Publisher::healthy(&pubr));
}
