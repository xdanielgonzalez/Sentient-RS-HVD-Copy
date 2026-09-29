use std::sync::atomic::Ordering;

use super::prelude::*;

impl JsonlHandle {
    /// Offer `record` to the writer task: a non-blocking `try_send`. Returns `true` if accepted
    /// into the channel, `false` if the channel was full (in which case `dropped` has already been
    /// incremented here). Never blocks the caller — see the `crate` module docs.
    pub fn offer(&self, record: LogRecord) -> bool {
        match self.tx.try_send(record) {
            Ok(()) => true,
            Err(_) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }
}

#[cfg(test)]
// Test code may unwrap/panic; the workspace lint gate denies these in production logic only.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;

    fn record(i: u64) -> LogRecord {
        LogRecord {
            ts_ms: i,
            bus: "pack-a".to_string(),
            topic: "tms/x".to_string(),
            payload: b"{}".to_vec(),
        }
    }

    #[test]
    fn offer_accepts_until_the_channel_is_full_then_drops_and_counts() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(2);
        let handle = JsonlHandle {
            tx,
            dropped: Arc::new(AtomicU64::new(0)),
        };
        assert!(handle.offer(record(0)));
        assert!(handle.offer(record(1)));
        // Channel depth 2, both slots full now (nothing has been received yet).
        assert!(!handle.offer(record(2)));
        assert_eq!(handle.dropped.load(Ordering::Relaxed), 1);
        // Draining a slot makes room again.
        let _ = rx.try_recv().expect("first record queued");
        assert!(handle.offer(record(3)));
        assert_eq!(handle.dropped.load(Ordering::Relaxed), 1);
    }
}
