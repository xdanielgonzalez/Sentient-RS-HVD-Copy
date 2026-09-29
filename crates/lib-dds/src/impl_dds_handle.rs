use super::prelude::*;

impl DdsHandle {
    /// Wrap a bounded sender. Prefer [`crate::dds_channel`], which pairs this with its receiver.
    pub(crate) fn new(tx: mpsc::Sender<DdsOutbound>) -> Self {
        Self { tx }
    }

    /// Enqueue an outbound publish, awaiting capacity (back-pressure). `Err(Closed)` if the stack is
    /// gone.
    pub async fn send(&self, out: DdsOutbound) -> Result<()> {
        self.tx.send(out).await.map_err(|_| DdsError::Closed)
    }

    /// Non-blocking enqueue. `Err(Closed)` if the queue is full or the stack is gone — the caller
    /// decides whether a dropped telemetry sample is acceptable (it usually is: the next poll
    /// supersedes it, mirroring the MQTT tap's fire-and-forget cadence).
    pub fn try_send(&self, out: DdsOutbound) -> Result<()> {
        self.tx.try_send(out).map_err(|_| DdsError::Closed)
    }
}
