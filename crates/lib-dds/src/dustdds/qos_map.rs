//! Translate a MIL-STD-3071 Annex D.5 QoS profile (`lib_tms::generated_qos`) into a dust_dds
//! `DataWriterQos`. NEVER a silent downgrade: any value the backend cannot represent returns
//! [`DdsError::UnsupportedQos`] (WP4 Risk 5). USER_DATA = the device identity (spec D.5).

use lib_tms::generated_qos::{
    DestinationOrder as TmsDestinationOrder, Durability as TmsDurability,
    Liveliness as TmsLiveliness, QosDuration, QosPolicies, QosProfile,
    Reliability as TmsReliability,
};
use lib_tms::Identity;

use dust_dds::infrastructure::qos::{DataReaderQos, DataWriterQos};
use dust_dds::infrastructure::qos_policy::{
    DataRepresentationQosPolicy, DeadlineQosPolicy, DestinationOrderQosPolicy,
    DestinationOrderQosPolicyKind, DurabilityQosPolicy, DurabilityQosPolicyKind, HistoryQosPolicy,
    HistoryQosPolicyKind, LivelinessQosPolicy, LivelinessQosPolicyKind, OwnershipStrengthQosPolicy,
    ReliabilityQosPolicy, ReliabilityQosPolicyKind, UserDataQosPolicy, XCDR2_DATA_REPRESENTATION,
};
use dust_dds::infrastructure::time::{Duration as DdsDuration, DurationKind};

use crate::types::{DdsError, Result};

/// dust_dds requires a finite `max_blocking_time` for a RELIABLE writer, but the D.5 data model does
/// not carry one (it is a runtime back-pressure bound, not a wire-compatibility policy). We set it
/// explicitly (never leaving a library default unexamined) to 100 ms — dust_dds's own default.
const RELIABLE_MAX_BLOCKING: DurationKind = DurationKind::Finite(DdsDuration::new(0, 100_000_000));

/// Build the `DataWriterQos` for `profile`, with `USER_DATA` = `device_id`.
pub fn to_writer_qos(profile: QosProfile, device_id: &Identity) -> Result<DataWriterQos> {
    policies_to_writer_qos(profile.policies(), device_id)
}

/// Build the `DataWriterQos` from a raw D.5 policy set (the [`to_writer_qos`] core). Separated so the
/// [`DdsError::UnsupportedQos`] validation paths are exercisable with values outside the 8 fixed
/// profiles (all of which happen to be representable today).
pub fn policies_to_writer_qos(p: QosPolicies, device_id: &Identity) -> Result<DataWriterQos> {
    let durability = DurabilityQosPolicy {
        kind: match p.durability {
            TmsDurability::Volatile => DurabilityQosPolicyKind::Volatile,
            TmsDurability::TransientLocal => DurabilityQosPolicyKind::TransientLocal,
        },
    };

    let reliability = ReliabilityQosPolicy {
        kind: match p.reliability {
            TmsReliability::BestEffort => ReliabilityQosPolicyKind::BestEffort,
            TmsReliability::Reliable => ReliabilityQosPolicyKind::Reliable,
        },
        max_blocking_time: RELIABLE_MAX_BLOCKING,
    };

    if p.history_depth == 0 {
        return Err(DdsError::UnsupportedQos {
            policy: "HISTORY",
            detail: "KEEP_LAST depth 0 is invalid".to_string(),
        });
    }
    let history = HistoryQosPolicy {
        kind: HistoryQosPolicyKind::KeepLast(p.history_depth),
    };

    let deadline = DeadlineQosPolicy {
        period: to_duration(p.deadline, "DEADLINE")?,
    };

    let liveliness = LivelinessQosPolicy {
        kind: match p.liveliness {
            TmsLiveliness::Automatic => LivelinessQosPolicyKind::Automatic,
        },
        lease_duration: DurationKind::Infinite,
    };

    let destination_order = DestinationOrderQosPolicy {
        kind: match p.destination_order {
            TmsDestinationOrder::ByReceptionTimestamp => {
                DestinationOrderQosPolicyKind::ByReceptionTimestamp
            }
        },
    };

    // The D.5 data model carries OWNERSHIP_STRENGTH but no ownership KIND; dust_dds's default
    // ownership (SHARED) stands, under which strength is inert — translated faithfully regardless.
    let ownership_strength = OwnershipStrengthQosPolicy {
        value: p.ownership_strength,
    };

    let user_data = UserDataQosPolicy {
        value: device_id.as_bytes().to_vec(),
    };

    // MIL-STD-3071 mandates XCDR2 on the wire — pin the writer's representation so dust_dds emits
    // D_CDR2_LE (matching our chains), never its Cdr1 default.
    let representation = DataRepresentationQosPolicy {
        value: vec![XCDR2_DATA_REPRESENTATION],
    };

    Ok(DataWriterQos {
        durability,
        reliability,
        history,
        deadline,
        liveliness,
        destination_order,
        ownership_strength,
        user_data,
        representation,
        ..Default::default()
    })
}

/// Build a matching `DataReaderQos` for `profile` (representation XCDR2, and durability/reliability/
/// history mirrored from the profile so a reader is QoS-compatible with the profile's writer).
pub fn to_reader_qos(profile: QosProfile) -> Result<DataReaderQos> {
    let p = profile.policies();
    if p.history_depth == 0 {
        return Err(DdsError::UnsupportedQos {
            policy: "HISTORY",
            detail: "KEEP_LAST depth 0 is invalid".to_string(),
        });
    }
    Ok(DataReaderQos {
        durability: DurabilityQosPolicy {
            kind: match p.durability {
                TmsDurability::Volatile => DurabilityQosPolicyKind::Volatile,
                TmsDurability::TransientLocal => DurabilityQosPolicyKind::TransientLocal,
            },
        },
        reliability: ReliabilityQosPolicy {
            kind: match p.reliability {
                TmsReliability::BestEffort => ReliabilityQosPolicyKind::BestEffort,
                TmsReliability::Reliable => ReliabilityQosPolicyKind::Reliable,
            },
            max_blocking_time: RELIABLE_MAX_BLOCKING,
        },
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(p.history_depth),
        },
        representation: DataRepresentationQosPolicy {
            value: vec![XCDR2_DATA_REPRESENTATION],
        },
        ..Default::default()
    })
}

/// Map a D.5 duration to a dust_dds `DurationKind`, rejecting non-finite/negative seconds.
fn to_duration(d: QosDuration, policy: &'static str) -> Result<DurationKind> {
    match d {
        QosDuration::Infinite => Ok(DurationKind::Infinite),
        QosDuration::Seconds(s) => {
            if !s.is_finite() || s < 0.0 {
                return Err(DdsError::UnsupportedQos {
                    policy,
                    detail: format!(
                        "duration {s}s is not a representable finite non-negative value"
                    ),
                });
            }
            let secs = s.trunc() as i32;
            let nanos = (s.fract() * 1_000_000_000.0).round() as u32;
            Ok(DurationKind::Finite(DdsDuration::new(secs, nanos)))
        }
    }
}
