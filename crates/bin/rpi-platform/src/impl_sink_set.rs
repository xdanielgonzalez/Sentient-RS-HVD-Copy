use super::prelude::*;

impl SinkSet {
    /// Offer `event` to every configured tap. See [`HostTap::offer`] — the per-variant forwarding
    /// itself is still a stub.
    pub fn offer(&mut self, event: &TapEvent<'_>) {
        for tap in &mut self.taps {
            tap.offer(event);
        }
    }
}
