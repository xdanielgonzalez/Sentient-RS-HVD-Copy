//! Compile-time field list + JSON emission primitives for the flat TMS-wire serializer.
//!
//! # Why type-level, not a runtime descriptor table
//!
//! A generated TMS struct's field names/order/optionality come from the IDL and must never drift
//! from it. Rather than a `&[FieldDescriptor]` runtime table walked in a loop (indirection, and
//! nothing stops a hand-edit from desyncing name/order/value), each field is its own zero-sized type
//! implementing [`Field<Src>`] with its wire `NAME`/`ORDER`/`KEY` as `const`s, and a struct's full
//! field list is a [`Nil`]/[`Node`] type-level cons list walked by [`FieldChain::emit_fields`]. The
//! whole chain inlines to a fixed, straight-line sequence of [`JsonSink`] calls at compile time — no
//! indirection, no possibility of the emitted order disagreeing with the `const`s. [`HasChain`] is
//! the generated-code entry point naming "the chain for this struct" without spelling out the full
//! nested `Node<…>` type at every call site.
//!
//! # Zero-copy
//!
//! [`Field::emit`] takes `src: &Src` and reads the value straight through the reference at emission
//! time — no intermediate struct, no owned copy, matching this crate's render-on-output memory model
//! (see the crate root doc comment).
//!
//! # This module, `lib-mapping`, and the layering split
//!
//! [`JsonSink`] and [`fixed_digits`] live here (not in `lib-mapping`, where the flat wire serializer
//! itself lives) because they have no canonical-model dependency — the leaf-crate property this
//! crate's root doc comment calls out. `lib-mapping`'s `CountingSink`/`WriteSink`
//! (`lib_mapping::wire`) implement [`JsonSink`] from here; their `f32`/`f64` methods are the only
//! place `ryu` enters this dependency graph, kept out of this `no_std`/no-alloc leaf crate entirely.

mod prelude;
pub mod types;

pub mod fixed_digits;

mod impl_nil;
mod impl_node;

pub use types::*;

/// Emit `src` as a complete JSON object (`'{' + fields + '}'`) built from field chain `C`. `C` is
/// not inferable from the arguments (nothing here is typed `C`), so callers supply it explicitly,
/// e.g. `emit_object::<Reading, <Reading as HasChain>::Chain, _>(src, sink)`.
#[inline(always)]
pub fn emit_object<Src: ?Sized, C: FieldChain<Src>, S: JsonSink>(src: &Src, sink: &mut S) {
    sink.str("{");
    let mut first = true;
    C::emit_fields(src, sink, &mut first);
    sink.str("}");
}

/// Emit a field key: a leading `,` unless this is the first field emitted into the current object
/// (tracked via `first`, which this then clears), followed by `key` (the pre-baked `"name":`
/// literal) verbatim.
#[inline(always)]
pub fn emit_key<S: JsonSink>(key: &'static str, sink: &mut S, first: &mut bool) {
    if !*first {
        sink.str(",");
    }
    sink.str(key);
    *first = false;
}

/// `serde_json_core` 0.6.0's per-`char` escape table (`src/ser/mod.rs:112-149`), indexed by the
/// control byte `0x00..=0x1F` it escapes: named two-character escapes (`\b \t \n \f \r`) at their
/// codepoints, `\u00XX` (uppercase hex) everywhere else in the range.
const CONTROL_ESCAPES: [&str; 32] = [
    "\\u0000", "\\u0001", "\\u0002", "\\u0003", "\\u0004", "\\u0005", "\\u0006", "\\u0007", "\\b",
    "\\t", "\\n", "\\u000B", "\\f", "\\r", "\\u000E", "\\u000F", "\\u0010", "\\u0011", "\\u0012",
    "\\u0013", "\\u0014", "\\u0015", "\\u0016", "\\u0017", "\\u0018", "\\u0019", "\\u001A",
    "\\u001B", "\\u001C", "\\u001D", "\\u001E", "\\u001F",
];

/// Emit `s` as a JSON string value: a leading `"`, `s`'s content with `\`/`"`/control characters
/// escaped, a trailing `"` — byte-identical to `serde_json_core`'s `&str`/`String` serialization
/// (`ser::Serializer::push_char`, `serde-json-core-0.6.0/src/ser/mod.rs:99-157`). Runs of bytes
/// needing no escape are passed to a single [`JsonSink::str`] call rather than one call per
/// character; splitting purely on byte value is safe here because every byte that triggers an
/// escape (`0x00..=0x1F`, `"` = `0x22`, `\` = `0x5C`) is ASCII, and UTF-8 continuation/lead bytes of
/// a multi-byte character are always `>= 0x80` — a multi-byte character can therefore never be
/// mistaken for (or split across) an escape trigger.
pub fn emit_str_value<S: JsonSink>(sink: &mut S, s: &str) {
    sink.str("\"");
    let bytes = s.as_bytes();
    let mut run_start = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        let esc = match b {
            b'\\' => "\\\\",
            b'"' => "\\\"",
            0x00..=0x1F => CONTROL_ESCAPES[b as usize],
            _ => continue,
        };
        if run_start < i {
            sink.str(&s[run_start..i]);
        }
        sink.str(esc);
        run_start = i + 1;
    }
    if run_start < bytes.len() {
        sink.str(&s[run_start..]);
    }
    sink.str("\"");
}

/// Emit a numeric field value per `v`'s variant: [`FloatVal::Fixed`] -> [`JsonSink::fixed`],
/// [`FloatVal::F32`]/[`FloatVal::F64`] -> the matching `ryu`-based sink method (non-finite -> `null`,
/// handled inside the sink impl), [`FloatVal::Null`] -> [`JsonSink::null`] directly — used for
/// fields with no data at all, with no float ever constructed.
#[inline(always)]
pub fn emit_num<S: JsonSink>(sink: &mut S, v: FloatVal) {
    match v {
        FloatVal::Fixed(f) => sink.fixed(f.scaled, f.frac_digits, f.invert_sign),
        FloatVal::F32(x) => sink.f32(x),
        FloatVal::F64(x) => sink.f64(x),
        FloatVal::Null => sink.null(),
    }
}
