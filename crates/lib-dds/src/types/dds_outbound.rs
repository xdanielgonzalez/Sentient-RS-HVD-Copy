use lib_tms::{Identity, TmsTopicId};

/// One queued DDS publish: a pre-serialized XCDR2 payload plus its routing key. Produced by the
/// poll-loop tap ([`super::DdsHandle`]), consumed by `run_dds_stack`, which lazily maps
/// `(device_id, topic)` to a `DataWriter` (USER_DATA = `device_id`).
#[derive(Debug, Clone)]
pub struct DdsOutbound {
    /// The DDS instance key = device identity. Also the per-writer `USER_DATA` (spec D.5).
    pub device_id: Identity,
    /// Which TMS topic this payload is for (selects the topic name, type, and D.5 QoS profile).
    pub topic: TmsTopicId,
    /// The complete RTPS SerializedPayload: encapsulation header + DHEADER + members, exactly as
    /// `lib_mapping::render_cdr` / `lib_tms` chains emit it.
    pub payload: Vec<u8>,
}
