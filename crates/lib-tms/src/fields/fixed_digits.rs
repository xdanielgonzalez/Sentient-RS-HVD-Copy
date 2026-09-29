//! Shared digit math for [`crate::fields::JsonSink::fixed`]/[`crate::fields::JsonSink::raw_int`] —
//! one set of functions every length-only and byte-emitting `JsonSink` impl (`lib_mapping::wire`'s
//! `CountingSink`/`WriteSink`) both call, so the counted length and the emitted byte count can never
//! drift apart (see `types/json_sink.rs`'s doc comment on the shared-traversal invariant).

/// Number of ASCII decimal digits in `magnitude` (`0` -> `1` digit, matching how `0` prints as a
/// single `'0'` character, not zero characters).
pub fn digit_count(magnitude: u64) -> usize {
    if magnitude == 0 {
        1
    } else {
        // `ilog10` is exact for `u64` (core, no_std, no float involved).
        magnitude.ilog10() as usize + 1
    }
}

/// Byte length of [`crate::fields::JsonSink::raw_int`]'s output for `i`: an optional `-` plus the
/// decimal digit count of `i`'s magnitude.
pub fn raw_int_len(i: i64) -> usize {
    let sign = usize::from(i < 0);
    // `i64::MIN.unsigned_abs()` is well-defined (unlike `-i64::MIN`, which overflows) — this is the
    // standard no-UB absolute-value-to-unsigned idiom.
    sign + digit_count(i.unsigned_abs())
}

/// Decompose a [`crate::fields::JsonSink::fixed`] call into `(negative, magnitude)`: `scaled`'s
/// sign after applying `invert_sign`, and its absolute value. Shared by the length and emit paths
/// so they can never disagree on the sign bit.
pub fn fixed_sign_and_magnitude(scaled: i64, invert_sign: bool) -> (bool, u64) {
    let effective = if invert_sign { -scaled } else { scaled };
    (effective < 0, effective.unsigned_abs())
}

/// Byte length of [`crate::fields::JsonSink::fixed`]'s output: optional `-`, integer-part digits
/// (with the mandatory leading `0` when the magnitude is smaller than `10^frac_digits`), a `.` (if
/// `frac_digits > 0`), and `frac_digits` fraction digits.
///
/// Takes `(negative, magnitude)` directly (rather than `scaled`/`invert_sign`) so a caller that
/// already ran [`fixed_sign_and_magnitude`] (e.g. the emit path, which needs the decomposed form
/// anyway to write the digits) doesn't recompute it — the length and emit paths share this one
/// function, so they can never disagree on byte count.
pub fn fixed_len(negative: bool, magnitude: u64, frac_digits: u8) -> usize {
    let sign = usize::from(negative);
    let scale = pow10(frac_digits);
    let int_part = magnitude / scale;
    let int_digits = digit_count(int_part); // >= 1, so the mandatory leading `0` falls out for free.
    if frac_digits == 0 {
        sign + int_digits
    } else {
        sign + int_digits + 1 /* '.' */ + frac_digits as usize
    }
}

/// `10^p` as `u64` (`p` stays small — at most a handful of fraction digits — so this never
/// overflows in this crate's use).
pub fn pow10(p: u8) -> u64 {
    let mut r: u64 = 1;
    let mut i = 0u8;
    while i < p {
        r *= 10;
        i += 1;
    }
    r
}

/// Max ASCII digits a `u64` magnitude can render as (`u64::MAX` = 20 digits) — sizes the fixed
/// stack scratch [`write_digits`]/`write_fixed_digits` format into before copying through a
/// [`crate::fields::JsonSink`], so no heap/`alloc` is ever needed for digit formatting.
pub const MAX_U64_DIGITS: usize = 20;

/// Render `magnitude`'s decimal digits (no sign) into `scratch` (must be >= [`MAX_U64_DIGITS`]
/// long), right-aligned, returning the occupied sub-slice. Shared by
/// [`crate::fields::JsonSink::raw_int`]'s emit path and `write_fixed_digits`'s integer-part
/// formatting.
pub fn write_digits(magnitude: u64, scratch: &mut [u8; MAX_U64_DIGITS]) -> &[u8] {
    if magnitude == 0 {
        scratch[MAX_U64_DIGITS - 1] = b'0';
        return &scratch[MAX_U64_DIGITS - 1..];
    }
    let mut n = magnitude;
    let mut i = MAX_U64_DIGITS;
    while n > 0 {
        i -= 1;
        scratch[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    &scratch[i..]
}
