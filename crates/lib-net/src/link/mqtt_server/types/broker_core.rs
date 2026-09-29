use super::super::{DEFAULT_MAX_CLIENTS, MAX_RETAINED, RX_CAP};
use super::active_topic::ActiveTopic;
use super::client_slot::ClientSlot;

/// The MQTT 3.1.1 broker core. Byte-in/byte-out and caller-driven — it does no socket I/O itself; a
/// firmware socket pump calls [`Self::feed_incoming`] with bytes read from each client and
/// [`Self::drain_outgoing`] to get bytes to write back, rendering payloads on demand via a
/// caller-provided closure. `MAX_CLIENTS` bounds the client table (default `DEFAULT_MAX_CLIENTS`
/// = 4). **Stores no payload bytes anywhere** — see the module docs' "Memory model" section.
pub struct BrokerCore<const MAX_CLIENTS: usize = DEFAULT_MAX_CLIENTS> {
    pub(in crate::link::mqtt_server) clients: [ClientSlot; MAX_CLIENTS],
    pub(in crate::link::mqtt_server) active: [Option<ActiveTopic>; MAX_RETAINED],
    /// Count of outgoing items dropped because a client's pending-publish list was full (or a
    /// control frame didn't fit). Never blocks, never panics — this counter is the only signal of
    /// backpressure.
    pub(in crate::link::mqtt_server) dropped: u32,
    /// Scratch reassembly snapshot reused by [`Self::feed_incoming`] (`RX_CAP` = 512 B), avoiding a
    /// stack-local `heapless::Vec` per packet-loop iteration on a stack-constrained embedded
    /// broker-pump caller (see the T3 RAM-budget/stack-overflow history in the Phase-2 plan).
    pub(in crate::link::mqtt_server) feed_scratch: heapless::Vec<u8, RX_CAP>,
}
