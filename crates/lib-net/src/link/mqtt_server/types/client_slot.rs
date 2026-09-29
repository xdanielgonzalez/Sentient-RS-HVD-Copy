use super::super::{FILTER_CAP, MAX_FILTERS_PER_CLIENT, MAX_PENDING_PER_CLIENT, RX_CAP};
use super::control_frame::ControlFrame;
use super::pending_publish::PendingPublish;

/// Per-client state: connection phase, subscriptions, and pending-publish (topic-only) queue.
pub(in crate::link::mqtt_server) struct ClientSlot {
    /// `true` once this slot has a live connection (set by [`BrokerCore::client_connected`]).
    pub(in crate::link::mqtt_server) occupied: bool,
    /// `true` once CONNECT has been accepted (CONNACK rc=0 staged) — SUBSCRIBE/PUBLISH-in are only
    /// honored after this.
    pub(in crate::link::mqtt_server) connected: bool,
    /// `true` once a protocol violation has doomed this connection (e.g. an unsupported CONNECT
    /// or malformed packet). Distinct from `occupied` going false: a refusal CONNACK may still be
    /// sitting in `control` waiting to be drained onto the wire, so internal error paths set this
    /// instead of tearing the slot down outright — [`BrokerCore::feed_incoming`] stops accepting
    /// further input once it's set, but [`BrokerCore::drain_outgoing`] still flushes `control`. The
    /// caller tears the slot down for real (freeing it for reuse) via
    /// [`BrokerCore::client_closed`] once it has finished draining and closed the socket.
    pub(in crate::link::mqtt_server) closing: bool,
    pub(in crate::link::mqtt_server) filters:
        heapless::Vec<heapless::String<FILTER_CAP>, MAX_FILTERS_PER_CLIENT>,
    pub(in crate::link::mqtt_server) rx: heapless::Vec<u8, RX_CAP>,
    /// At most one staged CONTROL packet at a time (CONNACK, or a SUBACK, or a PINGRESP) — none of
    /// this broker's control-packet-producing operations can be pipelined faster than the caller
    /// drains, since each is a direct response to one just-parsed inbound packet.
    pub(in crate::link::mqtt_server) control: Option<ControlFrame>,
    /// Topics awaiting a rendered PUBLISH (no payload bytes — see [`PendingPublish`]), drained one
    /// at a time by [`BrokerCore::drain_outgoing`].
    pub(in crate::link::mqtt_server) pending: heapless::Vec<PendingPublish, MAX_PENDING_PER_CLIENT>,
}
