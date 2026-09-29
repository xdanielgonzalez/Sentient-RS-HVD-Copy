//! `ryu`-based float formatting shared by [`crate::wire::CountingSink`]/[`crate::wire::WriteSink`]'s
//! `JsonSink::f32`/`f64` impls — same shared-traversal invariant as
//! `lib_tms::fields::fixed_digits` (both sinks format through these, so the counted length can
//! never desync from the emitted byte count), just homed in this crate rather than `lib-tms`: `ryu`
//! must never enter `lib-tms`'s no_std/no-alloc leaf dependency surface (see `crate::wire`'s module
//! doc comment).
//!
//! Non-finite handling (`NaN`/`±Infinity` -> JSON `null`, no `ryu` call at all) is the caller's job,
//! not this module's — see [`crate::wire::JsonSink::f32`]'s doc comment for the exact contract this
//! mirrors (`serde-json-core` 0.6.0's `Serializer::serialize_f32`/`serialize_f64`,
//! `src/ser/mod.rs:304-318`).

/// Format finite `v` via `ryu`'s shortest-round-trip decimal, passing the formatted digits to
/// `emit`. `ryu::Buffer` is stack scratch (like the `[u8; MAX_U64_DIGITS]` scratch
/// `lib_tms::fields::fixed_digits::write_digits` uses) — no heap, no persistent storage. Callers
/// must check `v.is_finite()` themselves; this function does not special-case non-finite values.
pub fn f32_ryu(v: f32, emit: impl FnOnce(&str)) {
    let mut buf = ryu::Buffer::new();
    emit(buf.format(v));
}

/// `f64` counterpart of [`f32_ryu`].
pub fn f64_ryu(v: f64, emit: impl FnOnce(&str)) {
    let mut buf = ryu::Buffer::new();
    emit(buf.format(v));
}
