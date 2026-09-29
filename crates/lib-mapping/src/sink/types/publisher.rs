use super::publish_error::PublishError;

/// A sink for MQTT publishes. Implemented by the RPi broker (publish onto the local broker) and the
/// Zephyr MQTT client (publish to a remote broker). `no_std`-safe signature.
pub trait Publisher {
    /// Publish `payload` to `topic` (QoS 1 where the impl supports it).
    fn publish(&mut self, topic: &str, payload: &[u8]) -> Result<(), PublishError>;
    /// True if the underlying link is healthy.
    fn healthy(&self) -> bool;
}
