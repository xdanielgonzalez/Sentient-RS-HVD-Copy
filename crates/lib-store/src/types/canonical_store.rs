use super::pending_topics::PendingTopics;
use heapless::HistoryBuf;
pub use lib_canonical::{CanonicalFault, CanonicalReading};

/// A bounded, overwrite-oldest ring buffer of [`CanonicalReading`]s (`DEPTH` slots), plus the
/// latest [`CanonicalFault`], plus which of the 3 TMS topics have new data waiting to be
/// published/retained.
///
/// Pure data structure: no `static`, no lock, no I/O. `push_reading`/`set_fault` are the
/// poll-loop-side write API; `latest_reading`/`latest_fault`/`take_pending` are the broker-pump-side
/// read API. In firmware (Step C) these two sides run on different threads and the store itself
/// will live behind a spinlock-guarded `static`; here, a single owner drives both sides directly
/// (as every host test in this module does), which is representative of the same read/write shape
/// without needing any concurrency machinery to prove it.
pub struct CanonicalStore<const DEPTH: usize> {
    pub(in super::super) readings: HistoryBuf<CanonicalReading, DEPTH>,
    pub(in super::super) fault: Option<CanonicalFault>,
    pub(in super::super) pending: PendingTopics,
}
