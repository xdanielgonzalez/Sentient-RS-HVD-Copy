/// Broker-lifecycle error (std side: starting/binding the embedded rumqttd broker).
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("mqtt broker error: {0}")]
    Link(String),
}
