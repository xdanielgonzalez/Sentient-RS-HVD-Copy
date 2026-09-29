use lib_tms::TmsTopicId;

/// A received DDS sample delivered to a subscriber's channel: the topic + the raw XCDR2
/// SerializedPayload as it arrived on the wire. Decoding to a canonical/TMS value is the caller's
/// job (mirrors the bytes-in publish seam — the crate stays payload-shape-agnostic).
///
/// The subscribe path (request→Reply listeners) is wired with the Pi integration in WP6; this type
/// pins its shape now so the seam ([`super::DdsParticipant::subscribe`]) is complete.
#[derive(Debug, Clone)]
pub struct DdsSample {
    /// The topic the sample arrived on.
    pub topic: TmsTopicId,
    /// The RTPS SerializedPayload bytes (encapsulation header + DHEADER + members).
    pub payload: Vec<u8>,
}
