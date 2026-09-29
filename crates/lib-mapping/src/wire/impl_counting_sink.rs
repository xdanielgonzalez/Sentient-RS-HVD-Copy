//! `impl JsonSink for CountingSink` — see `types/counting_sink.rs`'s doc comment.

use super::float_digits::{f32_ryu, f64_ryu};
use super::types::CountingSink;
use lib_tms::fields::fixed_digits::{fixed_len, fixed_sign_and_magnitude, raw_int_len};
use lib_tms::fields::JsonSink;

impl CountingSink {
    /// A fresh counter starting at zero.
    pub fn new() -> Self {
        CountingSink { len: 0 }
    }

    /// The accumulated length (bytes a [`crate::wire::WriteSink`] driven by the same serialize call
    /// would emit).
    pub fn len(&self) -> usize {
        self.len
    }

    /// True if nothing has been counted yet.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl JsonSink for CountingSink {
    fn str(&mut self, s: &str) {
        self.len += s.len();
    }

    fn raw_int(&mut self, i: i64) {
        self.len += raw_int_len(i);
    }

    fn fixed(&mut self, scaled: i64, frac_digits: u8, invert_sign: bool) {
        let (negative, magnitude) = fixed_sign_and_magnitude(scaled, invert_sign);
        self.len += fixed_len(negative, magnitude, frac_digits);
    }

    fn null(&mut self) {
        self.len += 4; // "null"
    }

    fn f32(&mut self, v: f32) {
        if v.is_finite() {
            f32_ryu(v, |s| self.len += s.len());
        } else {
            self.len += 4; // "null"
        }
    }

    fn f64(&mut self, v: f64) {
        if v.is_finite() {
            f64_ryu(v, |s| self.len += s.len());
        } else {
            self.len += 4; // "null"
        }
    }
}
