//! [`CdrChain`] — a compile-time, ordered list of [`super::CdrField`]s for a struct. See
//! `crate::cdr`'s module doc for the type-level-list rationale.

use super::cdr_sink::CdrSink;

/// A compile-time, IDL-ordered list of [`super::CdrField`]s for `Src`, built from
/// [`super::CdrNil`]/[`super::CdrNode`] cons cells. Walking the list is pure type-level recursion:
/// each [`CdrChain::emit`] resolves at compile time to a fixed, inlined sequence of
/// [`super::CdrField::emit`] calls — no runtime dispatch, no loop over a descriptor table, so the
/// emitted member order can never disagree with the IDL.
///
/// The same chain drives both the [`super::CdrCountingSink`] (to size a DHEADER) and the
/// [`super::CdrWriteSink`] (to emit the bytes) in [`crate::cdr::emit_dheader`], which is why the two
/// passes can never desync.
pub trait CdrChain<Src: ?Sized> {
    /// Member count in the chain (`CdrNil` = 0, each `CdrNode` adds 1) — mirrors the source IDL
    /// struct's member count.
    const LEN: usize;

    /// Serialize every member in the chain, in IDL declaration order, onto `sink`.
    fn emit<S: CdrSink>(src: &Src, sink: &mut S);
}
