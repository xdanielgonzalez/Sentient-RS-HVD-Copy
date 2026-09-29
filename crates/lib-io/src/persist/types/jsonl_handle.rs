use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

use super::log_record::LogRecord;

/// A cheap, cloneable handle for offering [`LogRecord`]s to the disk-logging
/// [`crate::persist::run_writer`] task. Cloned into every caller that wants to tail records to
/// disk (mirrors `lib_mqtt::broker::BrokerPublisher`'s share-by-clone shape).
#[derive(Clone)]
pub struct JsonlHandle {
    /// Bounded channel into the writer task (see `JsonlPolicy::queue_depth`).
    pub tx: Sender<LogRecord>,
    /// Count of records dropped because the channel was full (backpressure, never applied to the
    /// poll loop itself — see the `persist` module docs).
    pub dropped: Arc<AtomicU64>,
}
