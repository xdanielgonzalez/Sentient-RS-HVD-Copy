use super::json_sink::JsonSink;

/// A compile-time, ordered list of [`super::field::Field`]s for `Src` — built from
/// [`super::nil::Nil`]/[`super::node::Node`] type-level cons cells (see `crate::fields`'s module doc
/// comment). Walking the list is pure type-level recursion: each [`FieldChain::emit_fields`] call
/// resolves to a fixed, inlined sequence of [`super::field::Field::emit`] calls at compile time, no
/// runtime dispatch or loop over a descriptor table.
pub trait FieldChain<Src: ?Sized> {
    /// Field count in the chain (`Nil` = 0, each `Node` adds 1) — mirrors the source IDL struct's
    /// field count; used by generated code to size buffers/assert completeness.
    const LEN: usize;

    /// Emit every field in the chain, in order, onto `sink`. `first` tracks whether any field has
    /// been emitted yet into the enclosing object (see [`crate::fields::emit_key`]).
    fn emit_fields<S: JsonSink>(src: &Src, sink: &mut S, first: &mut bool);
}
