use rumqttd::local::LinkTx;
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// The running embedded MQTT server. Owns the broker's local Link sender.
pub struct LocalBroker {
    /// Thread running the broker. Shared so publishers can observe liveness via `is_finished`.
    pub(crate) broker_thread: Arc<JoinHandle<()>>,
    /// In-process link for publishing to the broker (shared via `Arc<Mutex>` since LinkTx is not Clone).
    pub(crate) link_tx: Arc<Mutex<LinkTx>>,
    /// Consecutive publish-failure counter, shared with every publisher this broker hands out.
    pub(crate) failures: Arc<AtomicU32>,
}
