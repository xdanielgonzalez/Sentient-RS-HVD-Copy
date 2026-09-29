//! [`CdrCountingSink`] — the length-only [`super::CdrSink`] impl. See `impl_cdr_counting_sink.rs`
//! for the behavior.

/// A [`super::CdrSink`] that emits nothing and only accumulates the byte count (with alignment
/// padding) each method call *would* have written. Used by [`crate::cdr::emit_dheader`] to compute a
/// subtree's serialized size for the DHEADER prefix in a bounded dual traversal (count, then write),
/// from the exact same call sequence the [`super::CdrWriteSink`] then executes — so the two can
/// never desync.
///
/// It is **constructible at a phase** ([`CdrCountingSink::at_phase`]): the accumulated `pos` is
/// seeded so that `pos % 4` equals a chosen alignment phase, which lets the alignment-phase property
/// test confirm that a DHEADER-delimited subtree's serialized size is position-independent (the
/// claim [`crate::cdr::emit_dheader`]'s "count from phase 0" shortcut relies on).
#[derive(Debug, Default, Clone, Copy)]
pub struct CdrCountingSink {
    pub(crate) pos: usize,
}
