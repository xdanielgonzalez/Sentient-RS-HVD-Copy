//! `JsonSink` — the write-target abstraction the flat TMS-JSON wire serializer writes through. One
//! trait, two impls in `lib-mapping` (`CountingSink`/`WriteSink`, `lib_mapping::wire`) executing the
//! *same* call sequence from the *same* serialize function, so a length computed by one can never
//! desync from bytes emitted by the other (see `docs/wire-serialization.md`'s "one shared
//! traversal" rationale). Homed in `lib-tms` (not `lib-mapping`) because it is a leaf abstraction —
//! no canonical-model dependency — shared by every wire encoder (see this crate's
//! `# This crate is a LEAF of the dependency graph` doc comment).
//!
//! No recursion, no heap, no `core::fmt::Write` machinery in the hot path — every method here is a
//! handful of digit computations plus (for a byte-emitting impl) a bounded-checked byte copy.

/// A write target for the flat TMS-JSON wire serializer. Every method appends the bytes/behavior
/// its name says; a length-only impl tallies the byte count without writing anything, a
/// byte-emitting impl writes the real bytes into a caller buffer — both from the exact same call
/// sequence (see this module's doc comment).
pub trait JsonSink {
    /// Append a raw string `s` verbatim (used for JSON structure: braces, commas, field-name/key
    /// literals, and pre-quoted string values the caller has already wrapped in `"…"`).
    fn str(&mut self, s: &str);

    /// Append a signed integer in plain decimal (no fraction, no leading zero beyond the value
    /// itself), e.g. `raw_int(-7)` -> `-7`, `raw_int(0)` -> `0`.
    fn raw_int(&mut self, i: i64);

    /// Append `scaled` (interpreted as the value multiplied by `10^frac_digits`) as a fixed-point
    /// decimal literal, with the decimal point placed positionally — no float formatting anywhere.
    ///
    /// `invert_sign` flips the sign of `scaled` before formatting (the TMS charge/discharge sign
    /// inversion, e.g. vendor `current_ca = -164` centi-amps with `invert_sign = true` renders
    /// `"1.64"`). Examples: `fixed(5452, 2, false)` -> `54.52`; `fixed(-164, 2, true)` -> `1.64`;
    /// `fixed(775, 3, false)` -> `0.775` (leading `0` when the magnitude is smaller than
    /// `10^frac_digits`); `fixed(0, 2, false)` -> `0.00`.
    fn fixed(&mut self, scaled: i64, frac_digits: u8, invert_sign: bool);

    /// Append the JSON literal `null` (used for permanently-`NaN` TMS fields and every
    /// `Option::None` field, matching `serde_json_core`'s `Option<T>` -> `null` encoding).
    fn null(&mut self);

    /// Append `v` as `ryu`'s shortest-round-trip decimal literal, e.g. `f32(54.52)` -> `54.52`.
    /// Non-finite (`NaN`/`+-Infinity`) writes the JSON `null` literal instead — never a float
    /// literal `ryu` cannot represent — matching `serde_json_core` 0.6.0's
    /// `Serializer::serialize_f32` (`if v.is_finite() { ryu } else { self.serialize_none() }`,
    /// `serde-json-core-0.6.0/src/ser/mod.rs:304-310`), the same non-finite contract already
    /// documented for the existing codec path (`bin-rpi-platform::impl_host_platform.rs:214-220`,
    /// "NaN fields serialize to JSON null, per the codec's non-finite handling"). Each impl performs
    /// the `ryu` call itself — this trait carries no `ryu` dependency.
    fn f32(&mut self, v: f32);

    /// `f64` counterpart of [`JsonSink::f32`] — same shortest-round-trip / non-finite-to-`null`
    /// contract, matching `serde_json_core`'s `Serializer::serialize_f64`.
    fn f64(&mut self, v: f64);
}
