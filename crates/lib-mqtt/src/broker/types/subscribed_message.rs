/// One received publish: the topic and payload of an inbound message, decoupled from rumqttd's
/// own `Notification`/`Publish` types so callers outside this crate never need that dependency.
/// Returned by `BrokerSubscriber::try_recv`.
#[derive(Debug, Clone)]
pub struct SubscribedMessage {
    pub topic: String,
    pub payload: Vec<u8>,
}
