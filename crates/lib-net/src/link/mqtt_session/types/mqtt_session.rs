use super::state::State;
use crate::link::mqtt_client::ConnectOptions;

/// Inbound reassembly capacity: the session only ever *receives* a CONNACK (4 bytes) and, once
/// connected, PINGRESP (2 bytes) at QoS 0 (no PUBACK). 64 bytes is comfortable slack for a coalesced
/// CONNACK+PINGRESP arriving in one transport read.
pub(in crate::link::mqtt_session) const RX_CAP: usize = 64;
/// Outbound staging capacity: one CONNECT plus one in-flight PUBLISH (topic + payload). Bounds the
/// largest payload a single owned client can stage before the caller drains it to the transport.
pub(in crate::link::mqtt_session) const TX_CAP: usize = 1024;

/// An owned MQTT 3.1.1 client session over a caller-driven byte transport. `'a` ties the borrowed
/// CONNECT fields (client id / credentials) for the session's lifetime.
pub struct MqttSession<'a> {
    pub(in crate::link::mqtt_session) opts: ConnectOptions<'a>,
    /// Keep-alive interval in milliseconds (0 disables keep-alive). Derived from
    /// `opts.keep_alive_secs`; a PINGREQ is staged once the link has been idle this long.
    pub(in crate::link::mqtt_session) keep_alive_ms: i64,
    pub(in crate::link::mqtt_session) state: State,
    /// `now_ms` of the last time bytes were drained to the transport — the keep-alive idle anchor.
    pub(in crate::link::mqtt_session) last_tx_ms: i64,
    /// Inbound bytes not yet consumed as a whole control packet.
    pub(in crate::link::mqtt_session) rx: heapless::Vec<u8, RX_CAP>,
    /// Outbound bytes staged for the caller to drain to the transport.
    pub(in crate::link::mqtt_session) tx: heapless::Vec<u8, TX_CAP>,
    /// How many bytes at the front of `tx` have already been drained (a cheap front cursor; `tx` is
    /// compacted/cleared once fully drained).
    pub(in crate::link::mqtt_session) tx_pos: usize,
}
