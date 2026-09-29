use crate::link::mqtt_client::MqttClientError;

/// A session-level (ordering/handshake) failure, distinct from a packet-encoding
/// [`MqttClientError`]. Fail-closed: any error means the session is not in a state to publish.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionError {
    /// [`crate::link::mqtt_session::MqttSession::publish`] was called before the broker accepted the connection (no CONNACK
    /// with return code 0 seen yet).
    NotConnected,
    /// Encoding a control packet failed (buffer/field-limit) — see [`MqttClientError`].
    Encode(MqttClientError),
    /// A staging or reassembly buffer overflowed (payload larger than the fixed capacity).
    BufferOverflow,
    /// The bytes where a CONNACK was expected did not parse as one.
    Malformed,
    /// The broker refused the connection: CONNACK carried a non-zero return code (1..=5 per
    /// MQTT 3.1.1 section 3.2.2.3). The code is preserved for the caller/log.
    Refused(u8),
}
