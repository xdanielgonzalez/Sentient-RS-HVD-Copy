//! [`HasCdrChain`] — the generated-code entry point binding a concrete type to its XCDR2
//! [`super::CdrChain`]. The CDR twin of [`crate::fields::HasChain`].

use super::cdr_chain::CdrChain;

/// Associates a source type with its compile-time XCDR2 [`CdrChain`] — the entry point generated
/// code (WP3 `generated_cdr.rs`) uses to name "the CDR field list for this struct" without spelling
/// out the full `CdrNode<F1, CdrNode<F2, …>>` chain type at every call site. One `impl` per
/// generated topic struct, emitted alongside the type's identity `…CdrSrc` binding, so
/// `crate::cdr::emit_object_cdr::<T, T::CdrChain, _>(value, sink)` renders any `T: HasCdrChain`.
pub trait HasCdrChain: Sized {
    /// The XCDR2 field chain naming every member of `Self`, in IDL declaration order.
    type CdrChain: CdrChain<Self>;
}
