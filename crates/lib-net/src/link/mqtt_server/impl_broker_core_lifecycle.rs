use super::prelude::*;

impl<const MAX_CLIENTS: usize> BrokerCore<MAX_CLIENTS> {
    /// An empty broker: no clients connected, no active topics.
    ///
    /// `BrokerCore` holds fixed-capacity client/topic tables sized for worst-case TMS/MQTT traffic
    /// (see the module docs), though considerably smaller now that no payload bytes are stored. A
    /// caller placing it in `static` storage on a stack-constrained target (the expected embedded
    /// usage: a firmware broker pump thread) should be aware this constructor returns `Self` **by
    /// value** — the caller is responsible for ensuring the temporary doesn't transit too small a
    /// stack on its way to its final resting place (host/test callers, and any caller with a
    /// generously sized stack, need not worry about this). `#[inline]` gives the optimizer the best
    /// chance to construct directly into a caller-owned destination (e.g. `ptr::write(dest,
    /// BrokerCore::new())`) rather than materializing a separate `BrokerCore`-sized stack temporary
    /// first.
    #[inline]
    pub fn new() -> Self {
        BrokerCore {
            clients: core::array::from_fn(|_| ClientSlot::new()),
            active: core::array::from_fn(|_| None),
            dropped: 0,
            feed_scratch: heapless::Vec::new(),
        }
    }

    /// How many outgoing items (pending-publish entries or control frames) have been dropped
    /// (staging overflow) since construction. Monotonic; the caller may sample it for diagnostics.
    pub fn dropped_count(&self) -> u32 {
        self.dropped
    }

    /// A new TCP connection has taken `slot` (the caller's accept-loop index). Resets any stale
    /// state left over from a previous occupant of the same slot.
    pub fn client_connected(&mut self, slot: usize) {
        if let Some(c) = self.clients.get_mut(slot) {
            c.reset();
            c.occupied = true;
        }
    }

    /// `slot`'s connection has closed (locally or by the peer). Frees the slot and drops its
    /// subscriptions; safe to call even if the slot was never connected.
    pub fn client_closed(&mut self, slot: usize) {
        if let Some(c) = self.clients.get_mut(slot) {
            c.reset();
        }
    }

    /// Internal: doom `slot`'s connection after a protocol violation (or a clean DISCONNECT)
    /// without discarding anything already staged as a control frame — a refusal CONNACK, for
    /// instance, must still reach the wire. Further [`Self::feed_incoming`] calls on this slot
    /// report [`BrokerError::InvalidSlot`] (via `is_connected`/occupied checks failing downstream),
    /// and the caller is expected to drain, then call the public [`Self::client_closed`] once the
    /// socket itself is torn down.
    pub(super) fn mark_closing(&mut self, slot: usize) {
        if let Some(c) = self.clients.get_mut(slot) {
            c.connected = false;
            c.closing = true;
            c.filters.clear();
            c.rx.clear();
            c.pending.clear();
        }
    }
}
