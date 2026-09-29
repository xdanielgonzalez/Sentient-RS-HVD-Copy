use lib_io::persist::JsonlHandle;

/// Disk / JSON-lines logging tap (Scope A) — offers each [`super::tap_event::TapEvent`] to the
/// jsonl writer task (`lib_io::persist::run_writer`) over `handle`.
#[derive(Clone)]
pub struct DiskTap {
    pub handle: JsonlHandle,
}
