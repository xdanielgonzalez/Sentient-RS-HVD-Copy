use rumqttd::local::LinkTx;
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// A [`lib_mapping::sink::Publisher`] backed by the embedded broker's in-process Link.
pub struct BrokerPublisher {
    /// In-process link sender for publishing to the broker (shared via `Arc<Mutex>`).
    pub(crate) link_tx: Arc<Mutex<LinkTx>>,
    /// Broker liveness handle (shared); `healthy` reports false if the broker thread has exited.
    pub(crate) broker_thread: Arc<JoinHandle<()>>,
    /// Consecutive publish failures; reset on any success.
    pub(crate) failures: Arc<AtomicU32>,
}
