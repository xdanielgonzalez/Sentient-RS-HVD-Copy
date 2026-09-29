use super::super::TOPIC_CAP;

/// One active-topic slot: a topic this broker has seen published at least once, and whether its
/// most recent publish was retained. **No payload bytes are stored here** — the render callback
/// passed to [`BrokerCore::drain_outgoing`] is the only source of payload bytes, called fresh for
/// every drain.
pub(in crate::link::mqtt_server) struct ActiveTopic {
    pub(in crate::link::mqtt_server) topic: heapless::String<TOPIC_CAP>,
    pub(in crate::link::mqtt_server) retained: bool,
}
