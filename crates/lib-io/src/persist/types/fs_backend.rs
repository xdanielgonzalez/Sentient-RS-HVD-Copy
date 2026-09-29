use std::fs::File;

/// Production [`super::write_backend::WriteBackend`]: a thin wrapper over `std::fs::File`, tracking
/// bytes written in-process (avoids a `stat` syscall on every [`WriteBackend::len`] call just to
/// answer how far into the file we are). Internal to this crate — callers never see the concrete
/// backend type, only `Box<dyn WriteBackend>` (see `types::jsonl_writer::JsonlWriter`).
pub(crate) struct FsBackend {
    pub(crate) file: File,
    pub(crate) len: u64,
}
