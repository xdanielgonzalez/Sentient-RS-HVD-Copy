use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

use super::condensed::Condensed;

/// A cheap, cloneable handle for offering [`Condensed`] frames to the [`crate::mesh::run_radio`] task
/// (mirrors `lib_persist::JsonlHandle`'s share-by-clone shape).
#[derive(Clone)]
pub struct RadioHandle {
    /// Bounded channel into the radio task.
    pub tx: Sender<Condensed>,
    /// Count of frames dropped because the channel was full (backpressure, never applied to the
    /// poll loop itself).
    pub dropped: Arc<AtomicU64>,
}
