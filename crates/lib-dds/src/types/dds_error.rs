use lib_tms::TmsTopicId;

/// A DDS transport failure. QoS gaps surface as [`DdsError::UnsupportedQos`] — the adapter NEVER
/// silently downgrades a MIL-STD-3071 Annex D.5 policy to a library default (WP4 Risk 5).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DdsError {
    /// A D.5 QoS policy value cannot be represented in the backend's QoS types. Carries the policy
    /// name + why. Never a silent downgrade.
    #[error("unsupported QoS policy `{policy}`: {detail}")]
    UnsupportedQos {
        policy: &'static str,
        detail: String,
    },

    /// The topic has no DDS type mapping in this build. The seam/adapter machinery is generic; the
    /// per-topic patch-type catalogue is populated incrementally (Heartbeat lands in WP4; the
    /// remaining published topics + request types are wired with the Pi taps in WP6).
    #[error("topic {0:?} has no DDS writer mapping in this build (added with the Pi taps, WP6)")]
    UnsupportedTopic(TmsTopicId),

    /// The DDS participant (or its transport) could not be created.
    #[error("participant/transport creation failed: {0}")]
    Participant(String),

    /// A `DataWriter` (topic + QoS) could not be created.
    #[error("writer creation failed for {topic:?}: {detail}")]
    WriterCreate { topic: TmsTopicId, detail: String },

    /// The pre-serialized XCDR2 payload could not be decoded into the topic's type (malformed bytes,
    /// or a patch-type/chain mismatch).
    #[error("payload decode failed: {0}")]
    Payload(String),

    /// The backend rejected the write (QoS blocking timeout, resource limits, deleted entity …).
    #[error("write failed: {0}")]
    Write(String),

    /// The outbound channel feeding the stack closed.
    #[error("dds stack channel closed")]
    Closed,
}
