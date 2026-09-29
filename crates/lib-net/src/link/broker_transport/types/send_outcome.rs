//! Outcome of one [`crate::link::broker_transport::BrokerConn::send_all`] call.

/// What a blocking, full-length `send_all` on a broker-pump client connection produced. Mirrors
/// the pump's existing `sent < out_n` check against `sz_sock_send`'s return
/// (`bin/zephyr/platform/src/broker_pump.rs`'s `rust_broker_main`): a short/negative count means the peer
/// is gone and the MQTT byte stream would desync, so the caller closes the slot rather than retry
/// or spin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendOutcome {
    /// The full buffer was sent (the impl is responsible for internally resuming across any
    /// legal short writes — see [`crate::link::broker_transport::BrokerConn::send_all`]'s doc comment;
    /// this variant is only returned once the whole length is confirmed sent).
    Sent,
    /// The peer is gone (reset/closed/a send-wait timeout) before the full buffer went out. The
    /// caller must close this slot — the byte stream is desynced from this point on.
    PeerGone,
}
