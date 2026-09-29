/// Broker-core failures. Fail-closed: any error from [`crate::link::BrokerCore::feed_incoming`] means the caller
/// should treat that client's connection as unusable (the slot is already closed internally).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerError {
    /// `slot` is out of range for this [`crate::link::BrokerCore`]'s `MAX_CLIENTS`.
    InvalidSlot,
    /// The bytes did not parse as a valid MQTT 3.1.1 control packet, or violated a bounded-capacity
    /// limit (topic/filter/client table too large). The slot has been closed.
    Malformed,
    /// The client sent CONNECT with an unsupported protocol name/level; a CONNACK with the correct
    /// refusal code was staged (if it fit) and the slot has been closed.
    UnsupportedProtocol,
}
