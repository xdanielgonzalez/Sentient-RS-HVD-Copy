use super::prelude::*;

/// Consecutive publish failures past which [`BrokerPublisher::healthy`] reports unhealthy.
const UNHEALTHY_AFTER_FAILURES: u32 = 3;

impl Publisher for BrokerPublisher {
    /// Publish a raw payload to the given topic via the in-process link. Maps the rumqttd link
    /// error onto the shared no_std `PublishError` (detail is logged, not carried). Updates the
    /// consecutive-failure counter that backs [`healthy`](Self::healthy).
    fn publish(&mut self, topic: &str, payload: &[u8]) -> Result<(), PublishError> {
        let mut link = match self.link_tx.lock() {
            Ok(l) => l,
            Err(_) => {
                self.failures.fetch_add(1, Ordering::Relaxed);
                return Err(PublishError::Sink);
            }
        };
        // rumqttd LinkTx requires owned topic/payload ('static).
        match link.publish(topic.to_string(), payload.to_vec()) {
            Ok(_) => {
                self.failures.store(0, Ordering::Relaxed);
                Ok(())
            }
            Err(e) => {
                self.failures.fetch_add(1, Ordering::Relaxed);
                log::warn!("broker publish failed on topic {topic}: {e}");
                Err(PublishError::Sink)
            }
        }
    }

    /// Healthy while the broker thread is alive and fewer than `UNHEALTHY_AFTER_FAILURES`
    /// publishes have failed back to back.
    fn healthy(&self) -> bool {
        !self.broker_thread.is_finished()
            && self.failures.load(Ordering::Relaxed) < UNHEALTHY_AFTER_FAILURES
    }
}
