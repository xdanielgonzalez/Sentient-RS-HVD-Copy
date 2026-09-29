//! [`CdrField`] — one compile-time XCDR2 field descriptor. See `crate::cdr`'s module doc for why
//! the field list is a type-level cons list rather than a runtime descriptor table.

use super::cdr_sink::CdrSink;

/// A single IDL-declared member of `Src`, knowing its declaration order and how to serialize its
/// own value into an XCDR2 stream. One `impl CdrField<Src>` per struct member is emitted by the IDL
/// codegen (WP3); [`crate::cdr::CdrNode`]/[`crate::cdr::CdrNil`] chain them into the full struct.
///
/// Members serialize in declaration order because all 136 TMS IDL types are `@extensibility
/// (APPENDABLE)` with **implicit** member IDs — the wire order *is* the declaration order
/// (DDS-XTypes 1.3 §7.4.3.5 rule (30): an APPENDABLE type serializes as `AsFinal`, i.e. member by
/// member with no per-member header).
pub trait CdrField<Src: ?Sized> {
    /// The member's zero-based declaration index in the source IDL — the single source of truth for
    /// serialization order (never re-derived from Rust struct field order, which is not guaranteed
    /// to be preserved).
    const ORDER: u16;

    /// Serialize this member's value onto `sink` (self-aligning per the member's XCDR2 type rules).
    /// An absent optional member routes through [`crate::cdr::emit_optional`]; a nested APPENDABLE
    /// member routes through [`crate::cdr::emit_dheader`].
    fn emit<S: CdrSink>(src: &Src, sink: &mut S);
}
