use super::super::TOPIC_CAP;

/// One pending-publish entry: a topic this client still needs a rendered PUBLISH for, and whether
/// it should carry the RETAIN flag (set for retained-replay-on-subscribe, per MQTT 3.1.1 section
/// 3.3.1.3; clear for a live publish).
pub(in crate::link::mqtt_server) struct PendingPublish {
    pub(in crate::link::mqtt_server) topic: heapless::String<TOPIC_CAP>,
    pub(in crate::link::mqtt_server) retain: bool,
}
