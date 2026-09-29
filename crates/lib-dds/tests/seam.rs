//! Seam unit tests (no network): the D.5 QoS → dust_dds translation is total + correct for the 8
//! profiles, surfaces [`DdsError::UnsupportedQos`] on non-representable values, and threads USER_DATA.
//! (Writer-map laziness is a `#[cfg(test)]` unit test in `stack.rs` — it needs the private `pump`.)
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use dust_dds::infrastructure::qos_policy::{
    DestinationOrderQosPolicyKind, DurabilityQosPolicyKind, HistoryQosPolicyKind,
    ReliabilityQosPolicyKind, XCDR2_DATA_REPRESENTATION,
};
use dust_dds::infrastructure::time::DurationKind;

use lib_dds::dustdds::qos_map::{policies_to_writer_qos, to_writer_qos};
use lib_tms::generated_qos::{
    qos_profile, DestinationOrder, Durability, Liveliness, QosDuration, QosPolicies, QosProfile,
    Reliability,
};
use lib_tms::{Identity, TmsTopicId};

fn ident(s: &str) -> Identity {
    let mut i = Identity::new();
    i.push_str(s).unwrap();
    i
}

/// Every one of the 8 named D.5 profiles translates without error, and the whole set of published
/// topics maps to a representable QoS (no silent gaps).
#[test]
fn all_profiles_and_topics_translate() {
    let dev = ident("METEOR-001");
    for profile in [
        QosProfile::PublishLast,
        QosProfile::Rare,
        QosProfile::Slow,
        QosProfile::Medium,
        QosProfile::Continuous,
        QosProfile::Command,
        QosProfile::Response,
        QosProfile::Reply,
    ] {
        let qos = to_writer_qos(profile, &dev)
            .unwrap_or_else(|e| panic!("profile {profile:?} must translate: {e}"));
        // Representation is always pinned to XCDR2 (MIL-STD-3071 mandate).
        assert_eq!(qos.representation.value, vec![XCDR2_DATA_REPRESENTATION]);
        // USER_DATA = deviceId on every writer.
        assert_eq!(qos.user_data.value, b"METEOR-001".to_vec());
    }

    // Exhaustive over TmsTopicId via qos_profile() — proves no published topic lands on an
    // unrepresentable QoS.
    for &topic in ALL_TOPICS {
        to_writer_qos(qos_profile(topic), &dev)
            .unwrap_or_else(|e| panic!("topic {topic:?} QoS must translate: {e}"));
    }
}

/// Spot-check the load-bearing profiles: Reply's KEEP_LAST 128 (the round's named requirement),
/// PublishLast's TRANSIENT_LOCAL + RELIABLE + KEEP_LAST 1, and a BEST_EFFORT + finite-deadline case.
#[test]
fn profile_policies_translate_faithfully() {
    let dev = ident("D");

    let reply = to_writer_qos(QosProfile::Reply, &dev).unwrap();
    assert!(matches!(
        reply.history.kind,
        HistoryQosPolicyKind::KeepLast(128)
    ));
    assert!(matches!(
        reply.reliability.kind,
        ReliabilityQosPolicyKind::Reliable
    ));
    assert!(matches!(
        reply.destination_order.kind,
        DestinationOrderQosPolicyKind::ByReceptionTimestamp
    ));

    let pl = to_writer_qos(QosProfile::PublishLast, &dev).unwrap();
    assert!(matches!(
        pl.durability.kind,
        DurabilityQosPolicyKind::TransientLocal
    ));
    assert!(matches!(
        pl.reliability.kind,
        ReliabilityQosPolicyKind::Reliable
    ));
    assert!(matches!(pl.history.kind, HistoryQosPolicyKind::KeepLast(1)));
    assert!(matches!(pl.deadline.period, DurationKind::Infinite));

    // Rare = VOLATILE + BEST_EFFORT + finite 2000 s deadline.
    let rare = to_writer_qos(QosProfile::Rare, &dev).unwrap();
    assert!(matches!(
        rare.durability.kind,
        DurabilityQosPolicyKind::Volatile
    ));
    assert!(matches!(
        rare.reliability.kind,
        ReliabilityQosPolicyKind::BestEffort
    ));
    match rare.deadline.period {
        DurationKind::Finite(d) => assert_eq!(d.sec(), 2000),
        DurationKind::Infinite => panic!("Rare deadline must be finite"),
    }
}

/// Non-representable policy values surface `UnsupportedQos` — never a silent downgrade (Risk 5). The 8
/// fixed profiles are all representable, so we exercise the guards through the raw-policy entry point.
#[test]
fn unsupported_qos_is_surfaced_not_downgraded() {
    let dev = ident("D");
    let base = QosProfile::Medium.policies();

    // KEEP_LAST depth 0 is invalid.
    let bad_depth = QosPolicies {
        history_depth: 0,
        ..base
    };
    let err = policies_to_writer_qos(bad_depth, &dev).unwrap_err();
    assert!(
        matches!(&err, lib_dds::DdsError::UnsupportedQos { policy, .. } if *policy == "HISTORY"),
        "depth 0 must be UnsupportedQos(HISTORY), got {err}"
    );

    // A negative deadline is not a representable duration.
    let neg_deadline = QosPolicies {
        deadline: QosDuration::Seconds(-1.0),
        ..base
    };
    let err = policies_to_writer_qos(neg_deadline, &dev).unwrap_err();
    assert!(
        matches!(&err, lib_dds::DdsError::UnsupportedQos { policy, .. } if *policy == "DEADLINE"),
        "negative deadline must be UnsupportedQos(DEADLINE), got {err}"
    );

    // NaN likewise.
    let nan_deadline = QosPolicies {
        deadline: QosDuration::Seconds(f32::NAN),
        ..base
    };
    assert!(matches!(
        policies_to_writer_qos(nan_deadline, &dev),
        Err(lib_dds::DdsError::UnsupportedQos {
            policy: "DEADLINE",
            ..
        })
    ));
}

/// A hand-built representable policy set round-trips its enum kinds (sanity that the match arms map
/// the right variants, independent of the fixed profiles).
#[test]
fn representable_policies_map_each_kind() {
    let dev = ident("D");
    let p = QosPolicies {
        durability: Durability::TransientLocal,
        reliability: Reliability::Reliable,
        history_depth: 7,
        deadline: QosDuration::Infinite,
        ownership_strength: 3,
        liveliness: Liveliness::Automatic,
        destination_order: DestinationOrder::ByReceptionTimestamp,
    };
    let qos = policies_to_writer_qos(p, &dev).unwrap();
    assert!(matches!(
        qos.history.kind,
        HistoryQosPolicyKind::KeepLast(7)
    ));
    assert_eq!(qos.ownership_strength.value, 3);
}

/// Every `TmsTopicId` variant — kept exhaustive so a new topic forces an update here (mirrors the
/// generated `qos_profile` match).
const ALL_TOPICS: &[TmsTopicId] = &[
    TmsTopicId::ControlHardwareUpdate,
    TmsTopicId::PowerHardwareUpdate,
    TmsTopicId::ClockState,
    TmsTopicId::GroundingCircuitState,
    TmsTopicId::ActiveDiagnosticState,
    TmsTopicId::AuthorizationToEnergizeResult,
    TmsTopicId::AuthorizationToEnergizeRequest,
    TmsTopicId::AuthorizationToEnergizeReply,
    TmsTopicId::IdentityNicknameRequest,
    TmsTopicId::DeviceIcon,
    TmsTopicId::ControlParameterRequest,
    TmsTopicId::ControlParameterState,
    TmsTopicId::PowerPortState,
    TmsTopicId::IdentityNicknameState,
    TmsTopicId::GroundingCircuitRequest,
    TmsTopicId::Heartbeat,
    TmsTopicId::MetricParameterState,
    TmsTopicId::OperatorIntentRequest,
    TmsTopicId::OperatorIntentState,
    TmsTopicId::OperatorPowerConnectionState,
    TmsTopicId::DiscoveredPowerConnectionState,
    TmsTopicId::MicrogridPowerConnectionState,
    TmsTopicId::PowerSwitchRequest,
    TmsTopicId::Reply,
    TmsTopicId::EnergyStartStopRequest,
    TmsTopicId::EnergyStartStopState,
    TmsTopicId::ActiveMicrogridControllerState,
    TmsTopicId::AcMeasurementUpdate,
    TmsTopicId::AcSummaryMeasurementUpdate,
    TmsTopicId::AcLoadSharingRequest,
    TmsTopicId::AcLoadSharingState,
    TmsTopicId::DcMeasurementUpdate,
    TmsTopicId::DcSummaryMeasurementUpdate,
    TmsTopicId::DcLoadSharingRequest,
    TmsTopicId::DcLoadSharingState,
    TmsTopicId::StorageUpdate,
    TmsTopicId::DeviceInfo,
];
