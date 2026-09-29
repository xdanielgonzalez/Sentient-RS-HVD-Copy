use super::disk_state::DiskState;
use super::jsonl_policy::JsonlPolicy;
use super::write_backend::WriteBackend;

/// Owns the currently-open jsonl file (via the [`WriteBackend`] seam) plus rotation/disk-full
/// bookkeeping. Driven by [`crate::persist::run_writer`]; constructed via `JsonlWriter::new` (see
/// `impl_jsonl_writer.rs`) — internal to this crate, never constructed by other crates directly
/// (they drive the writer only through `crate::persist::run_writer` + `JsonlHandle`).
pub struct JsonlWriter {
    pub policy: JsonlPolicy,
    pub state: DiskState,
    /// The currently-open backend, if a file has been opened yet. `None` before the first write,
    /// and again immediately after an open/write failure — the next write attempt reopens it (see
    /// `impl_jsonl_writer.rs::ensure_open`).
    pub backend: Option<Box<dyn WriteBackend>>,
}
