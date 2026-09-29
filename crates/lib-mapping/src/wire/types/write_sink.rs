//! [`WriteSink`] — the byte-emitting [`crate::wire::JsonSink`] impl. See
//! [`lib_tms::fields::JsonSink`]'s doc comment for the shared-traversal invariant this
//! participates in.

/// A [`crate::wire::JsonSink`] that writes real bytes into a caller-owned `&mut [u8]`, bounds-
/// checked on every write. If a write would overflow `buf`, it is skipped (never a partial write,
/// never a panic/OOB) and [`WriteSink::overflowed`] latches `true` for the rest of the call — the
/// driving `serialize_into` function checks this once at the end and reports `None`/an error rather
/// than returning a truncated, invalid buffer length.
pub struct WriteSink<'a> {
    pub(crate) buf: &'a mut [u8],
    pub(crate) pos: usize,
    pub(crate) overflowed: bool,
}
