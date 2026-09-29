use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use lib_io::persist::RawCaptureRecord;
use tokio::sync::mpsc::Sender;

/// A cheap, cloneable handle for offering [`RawCaptureRecord`]s to the raw pre-parse wire-capture
/// [`lib_io::persist::run_capture_writer`] task — the [`super::capture_serial::CaptureSerial`]
/// decorator's counterpart to `JsonlHandle`/`RadioHandle` (same share-by-clone shape; see
/// `lib_io::persist::JsonlHandle`). One handle is minted per process (one writer task, one channel —
/// see `bin_rpi::runtime::supervise`) and cloned into every configured bus's `CaptureSerial`, each
/// stamping its own `bus` label into every record it offers.
#[derive(Clone)]
pub struct CaptureHandle {
    /// Bounded channel into the writer task (see `CaptureConfig::queue_depth`).
    pub tx: Sender<RawCaptureRecord>,
    /// Count of exchanges dropped because the channel was full or the writer task had already
    /// exited — never applied as backpressure to the poll loop itself (mirrors
    /// `JsonlHandle::dropped`).
    pub dropped: Arc<AtomicU64>,
    /// This bus's label (mirrors `RawCaptureRecord::bus`/`HostPlatform::bus`), stamped into every
    /// record this handle offers.
    pub bus: String,
}
