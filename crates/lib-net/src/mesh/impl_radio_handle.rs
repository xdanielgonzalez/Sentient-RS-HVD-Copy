use super::prelude::*;
use std::sync::atomic::Ordering;

impl RadioHandle {
    /// Offer `item` to the radio task — non-blocking, infallible by contract (mirrors
    /// `impl_app::host::SinkSet::offer`'s "never blocks the poll loop" rule, and the identical
    /// try-send pattern `crates/bin/rpi-platform/src/impl_host_tap.rs`'s `HostTap::Mesh` dispatch
    /// already applies inline). Returns `true` if the frame was queued; a full channel increments
    /// `dropped` and returns `false` rather than backpressuring the caller.
    pub fn offer(&self, item: Condensed) -> bool {
        match self.tx.try_send(item) {
            Ok(()) => true,
            Err(_) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }
}

#[cfg(test)]
// Test code may unwrap/expect/panic; the workspace lint gate denies these in production logic
// only.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;

    #[test]
    fn offer_succeeds_and_leaves_dropped_at_zero_when_there_is_room() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let handle = RadioHandle {
            tx,
            dropped: Arc::new(AtomicU64::new(0)),
        };
        assert!(handle.offer(Condensed::default()));
        assert_eq!(handle.dropped.load(Ordering::Relaxed), 0);
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn offer_increments_dropped_and_returns_false_when_the_channel_is_full() {
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let handle = RadioHandle {
            tx,
            dropped: Arc::new(AtomicU64::new(0)),
        };
        assert!(handle.offer(Condensed::default()));
        // The channel now holds one queued item and has no consumer draining it, so the second
        // offer must be rejected rather than blocking.
        assert!(!handle.offer(Condensed::default()));
        assert_eq!(handle.dropped.load(Ordering::Relaxed), 1);
    }
}
