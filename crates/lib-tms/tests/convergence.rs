//! CONVERGENCE / API SURFACE — the generated catalogue reproduces the golden hand-written schema
//! shape and exposes the topic identity + factory surface. Byte-level field-order equivalence is proven in
//! `serde_byte_identity.rs`; this file checks the trait/factory/topic-id contract.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::all)]

use lib_tms::generated as g;
use lib_tms::{TmsTopic, TmsTopicId};

/// Every golden STOR/breadth type is present in the generated catalogue (referencing the type names
/// compiles only if the generator emitted them). The IDL is a SUPERSET (103 structs / 33 enums).
#[test]
fn generated_is_a_superset_of_golden() {
    fn assert_type<T>() {}
    // enums
    assert_type::<g::CableSenseStatus>();
    assert_type::<g::CircuitContinuity>();
    assert_type::<g::DtcSeverity>();
    assert_type::<g::FailureModeIndicator>();
    assert_type::<g::DeviceRole>();
    assert_type::<g::EnergyStartStopLevel>();
    assert_type::<g::ReplyCode>();
    // structs (STOR + breadth)
    assert_type::<g::PowerHardwareUpdate>();
    assert_type::<g::ActiveDiagnosticState>();
    assert_type::<g::EnergyStorageState>();
    assert_type::<g::StorageCellState>();
    assert_type::<g::DiagnosticTroubleCode>();
    assert_type::<g::ClockMonotonic>();
    assert_type::<g::Heartbeat>();
    assert_type::<g::StorageUpdate>();
    assert_type::<g::DeviceInfo>();
    assert_type::<g::Reply>();
    assert_type::<g::EnergyStartStopState>();
    assert_type::<g::PowerPortState>();
    assert_type::<g::ControlHardwareUpdate>();
    assert_type::<g::Curve2D>();
    assert_type::<g::Point2D>();
    assert_type::<g::ProductInfo>();
    assert_type::<g::TopicInfo>();
    // ac/dc collisions get Dc/Ac-prefixed names.
    assert_type::<g::DcMeasurementUpdate>();
    assert_type::<g::AcMeasurementUpdate>();
    assert_type::<g::DcPowerLineMeasurement>();
    assert_type::<g::AcPowerLineMeasurement>();
}

/// `TmsTopic` carries the IDL provenance + the 1-byte identity.
#[test]
fn tms_topic_identity_and_provenance() {
    assert_eq!(<g::StorageUpdate as TmsTopic>::TOPIC, "StorageUpdate");
    assert_eq!(
        <g::StorageUpdate as TmsTopic>::TOPIC_ID,
        TmsTopicId::StorageUpdate
    );
    assert_eq!(<g::StorageUpdate as TmsTopic>::IDL_LINE, 5113);
    assert_eq!(<g::StorageUpdate as TmsTopic>::topic_str(), "StorageUpdate");

    assert_eq!(
        <g::PowerHardwareUpdate as TmsTopic>::TOPIC_ID,
        TmsTopicId::PowerHardwareUpdate
    );
    assert_eq!(
        <g::ActiveDiagnosticState as TmsTopic>::TOPIC,
        "ActiveDiagnosticState"
    );

    // Multi-topic type serves both its topics; TOPIC is the primary.
    assert_eq!(
        <g::DcMeasurementUpdate as TmsTopic>::TOPIC,
        "DcMeasurementUpdate"
    );
    assert_eq!(
        <g::DcMeasurementUpdate as TmsTopic>::topics(),
        &["DcMeasurementUpdate", "DcSummaryMeasurementUpdate"]
    );
    assert_eq!(
        <g::PowerConnectionState as TmsTopic>::topics(),
        &[
            "OperatorPowerConnectionState",
            "DiscoveredPowerConnectionState",
            "MicrogridPowerConnectionState"
        ]
    );
}

/// Topic identity is a compact index; the string resolves at the boundary for the shipping topics.
#[test]
fn topic_id_resolves_shipping_topics() {
    assert_eq!(
        TmsTopicId::from_str("DcMeasurementUpdate"),
        Some(TmsTopicId::DcMeasurementUpdate)
    );
    assert_eq!(
        TmsTopicId::from_str("PowerHardwareUpdate"),
        Some(TmsTopicId::PowerHardwareUpdate)
    );
    assert_eq!(
        TmsTopicId::from_str("ActiveDiagnosticState"),
        Some(TmsTopicId::ActiveDiagnosticState)
    );
    // The summary alias resolves to its own index (round-trips), distinct from the base topic.
    assert_eq!(
        TmsTopicId::from_str("DcSummaryMeasurementUpdate"),
        Some(TmsTopicId::DcSummaryMeasurementUpdate)
    );
    assert_ne!(
        TmsTopicId::DcSummaryMeasurementUpdate,
        TmsTopicId::DcMeasurementUpdate
    );
    // Topic identity is a stable 1-byte index.
    assert_eq!(core::mem::size_of::<TmsTopicId>(), 1);
    assert!((TmsTopicId::DcMeasurementUpdate as u8) < TmsTopicId::COUNT as u8);
}

/// `TmsTopicId::as_str`/`from_str` round-trip for EVERY topic (the "bring the index back" law).
#[test]
fn topic_id_as_str_from_str_round_trip_for_every_topic() {
    assert!(TmsTopicId::COUNT >= 30, "expected the full topic set");
    assert_eq!(TmsTopicId::ALL.len(), TmsTopicId::COUNT);
    for &id in TmsTopicId::ALL {
        let s = id.as_str();
        // from_str brings the index back exactly.
        assert_eq!(TmsTopicId::from_str(s), Some(id), "round-trip for {s}");
        // and the discriminant is a stable 1-byte index.
        assert!((id as u8) < TmsTopicId::COUNT as u8);
    }
    // Distinct discriminants + distinct strings.
    let mut strs: Vec<&str> = TmsTopicId::ALL.iter().map(|id| id.as_str()).collect();
    let total = strs.len();
    strs.sort();
    strs.dedup();
    assert_eq!(strs.len(), total, "topic strings are distinct");
    assert_eq!(TmsTopicId::from_str("NotATopic"), None);
}

/// The `TmsMessage` factory decodes inbound bytes by topic and round-trips (host-only serde path).
#[cfg(feature = "std")]
#[test]
fn factory_decode_encode_round_trip() {
    use lib_tms::TmsMessage;

    let mut device_id = heapless::String::<32>::new();
    device_id.push_str("METEOR-1").unwrap();
    let hb = g::Heartbeat {
        device_id,
        sequence_number: 7,
    };
    let bytes = serde_json_core::to_string::<_, 256>(&hb).unwrap();

    let msg = TmsMessage::decode("Heartbeat", bytes.as_bytes()).expect("decode");
    assert_eq!(msg.topic(), TmsTopicId::Heartbeat);
    assert_eq!(msg.topic_str(), "Heartbeat");
    assert_eq!(msg.idl_line(), 3672);
    match &msg {
        TmsMessage::Heartbeat(h) => assert_eq!(h.sequence_number, 7),
        _ => panic!("wrong variant"),
    }
    // Re-encode and confirm the payload survives.
    let re = msg.encode().expect("encode");
    let msg2 = TmsMessage::decode("Heartbeat", re.as_bytes()).expect("decode2");
    assert_eq!(msg, msg2);

    // A multi-topic alias decodes to the base type's variant.
    let dc_bytes = TmsMessage::decode("DcSummaryMeasurementUpdate", b"{\"device_id\":\"D\",\"time_measured\":{\"seconds\":0,\"nanoseconds\":0},\"external_measurement\":[],\"internal_measurement\":[]}");
    assert!(matches!(dc_bytes, Ok(TmsMessage::DcMeasurementUpdate(_))));

    // An unknown topic is rejected.
    assert!(TmsMessage::decode("NopeTopic", b"{}").is_err());
}
