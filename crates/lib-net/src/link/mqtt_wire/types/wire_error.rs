/// MQTT control-packet encode/parse failures. Fail-closed: any error means the packet is not
/// emitted / not trusted. Re-used as [`crate::link::mqtt_client::MqttClientError`] (a type alias) so
/// existing call sites are unaffected by the extraction.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireError {
    /// The caller's output buffer was too small for the encoded packet.
    BufferTooSmall,
    /// A field (client id, topic, ...) exceeded the MQTT 3.1.1 wire limits (a 16-bit length).
    FieldTooLong,
    /// The bytes did not parse as the expected packet type, or violated a spec-fixed limit (e.g. a
    /// Remaining Length varint with more than 4 continuation bytes) — a genuine protocol violation,
    /// distinct from [`WireError::Truncated`] (which just needs more bytes buffered).
    Malformed,
    /// The bytes buffered so far are a valid *prefix* of a packet but end before the value they're
    /// decoding is complete — the caller should wait for more input, not treat this as malformed.
    Truncated,
    /// QoS 1/2 PUBLISH requires a packet id (MQTT 3.1.1 section 3.3.2.2); none was given.
    MissingPacketId,
}
