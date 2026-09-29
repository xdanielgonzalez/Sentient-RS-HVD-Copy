use super::json_sink::JsonSink;

/// A single compile-time field descriptor: one IDL-declared field of `Src`, carrying its wire
/// name/order as `const`s and knowing how to write its own value. Implementations are emitted by
/// the IDL codegen (one `impl Field<Src> for <FieldMarker>` per struct field) — see
/// `crate::fields`'s module doc comment for why this is a type-level list rather than a runtime
/// loop over a descriptor table.
pub trait Field<Src: ?Sized> {
    /// The field's declaration index in the source IDL — the single source of truth for wire
    /// emission order (never re-derived from struct field order, which Rust does not guarantee is
    /// preserved).
    const ORDER: u16;

    /// The wire key (bare JSON object field name), e.g. `"device_id"`.
    const NAME: &'static str;

    /// The pre-baked, ready-to-emit key literal: `NAME` quoted plus the trailing colon, e.g.
    /// `"\"device_id\":"`. Baking this at compile time turns each field's key emission into one
    /// [`JsonSink::str`] call instead of assembling quotes/colon per call.
    const KEY: &'static str;

    /// Emit this field's contribution to the enclosing JSON object: if the value is present, calls
    /// [`crate::fields::emit_key`] with [`Field::KEY`] then writes the value through `sink`. Absent
    /// `Option` fields emit NOTHING (the omit policy) and must leave `*first` untouched.
    fn emit<S: JsonSink>(src: &Src, sink: &mut S, first: &mut bool);
}
