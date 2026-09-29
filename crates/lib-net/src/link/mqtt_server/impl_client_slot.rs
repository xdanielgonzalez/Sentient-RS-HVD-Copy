use super::prelude::*;

impl ClientSlot {
    pub(super) const fn new() -> Self {
        ClientSlot {
            occupied: false,
            connected: false,
            closing: false,
            filters: heapless::Vec::new(),
            rx: heapless::Vec::new(),
            control: None,
            pending: heapless::Vec::new(),
        }
    }

    pub(super) fn reset(&mut self) {
        self.occupied = false;
        self.connected = false;
        self.closing = false;
        self.filters.clear();
        self.rx.clear();
        self.control = None;
        self.pending.clear();
    }

    pub(super) fn rx_consume(&mut self, n: usize) {
        let len = self.rx.len();
        if n >= len {
            self.rx.clear();
            return;
        }
        self.rx.copy_within(n.., 0);
        self.rx.truncate(len - n);
    }

    /// Stage a CONTROL packet, overwriting any prior unset one is not expected to happen (each is a
    /// direct reply to a just-parsed inbound packet, and `drain_outgoing` always drains `control`
    /// before parsing continues) — but if it somehow would, the newer one wins rather than growing
    /// unboundedly.
    pub(super) fn stage_control(&mut self, bytes: &[u8]) -> Result<(), ()> {
        let frame = heapless::Vec::from_slice(bytes).map_err(|_| ())?;
        self.control = Some(ControlFrame { bytes: frame });
        Ok(())
    }

    /// Enqueue a pending publish; returns `Err(())` on overflow (caller counts the drop).
    pub(super) fn push_pending(&mut self, topic: &str, retain: bool) -> Result<(), ()> {
        let owned = heapless::String::try_from(topic).map_err(|_| ())?;
        self.pending
            .push(PendingPublish {
                topic: owned,
                retain,
            })
            .map_err(|_| ())
    }
}
