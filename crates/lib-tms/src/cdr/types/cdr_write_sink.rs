//! [`CdrWriteSink`] — the byte-emitting [`super::CdrSink`] impl. See `impl_cdr_write_sink.rs` for
//! the behavior and `types/cdr_sink.rs` for the shared-traversal invariant it participates in.

/// A [`super::CdrSink`] that writes real XCDR2 bytes into a caller-owned `&mut [u8]`, bounds-checked
/// on every write. If a write would overflow `buf`, it is skipped (never a partial write, never a
/// panic/OOB) and [`CdrWriteSink::overflowed`] latches `true` for the rest of the serialization —
/// the driving encoder checks this once at the end and reports failure rather than returning a
/// truncated, invalid payload. This latch-on-overflow contract mirrors `lib_mapping::wire::WriteSink`.
pub struct CdrWriteSink<'a> {
    pub(crate) buf: &'a mut [u8],
    pub(crate) pos: usize,
    pub(crate) overflowed: bool,
}
