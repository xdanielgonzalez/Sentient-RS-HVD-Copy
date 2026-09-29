//! `impl JsonSink for WriteSink` — see `types/write_sink.rs`'s doc comment.

use super::float_digits::{f32_ryu, f64_ryu};
use super::types::WriteSink;
use lib_tms::fields::fixed_digits::{
    fixed_sign_and_magnitude, pow10, write_digits, MAX_U64_DIGITS,
};
use lib_tms::fields::JsonSink;

impl<'a> WriteSink<'a> {
    /// Wrap `buf` for writing from the start.
    pub fn new(buf: &'a mut [u8]) -> Self {
        WriteSink {
            buf,
            pos: 0,
            overflowed: false,
        }
    }

    /// Bytes written so far (only meaningful if [`WriteSink::overflowed`] is `false`).
    pub fn len(&self) -> usize {
        self.pos
    }

    /// True if nothing has been written yet.
    pub fn is_empty(&self) -> bool {
        self.pos == 0
    }

    /// True if any write since construction was skipped because `buf` ran out of room.
    pub fn overflowed(&self) -> bool {
        self.overflowed
    }

    /// Append raw bytes, bounds-checked; sets [`WriteSink::overflowed`] and writes nothing if `bytes`
    /// would not fit in the remaining capacity (never a partial write).
    pub(crate) fn push_bytes(&mut self, bytes: &[u8]) {
        let end = self.pos + bytes.len();
        match self.buf.get_mut(self.pos..end) {
            Some(dst) => {
                dst.copy_from_slice(bytes);
                self.pos = end;
            }
            None => self.overflowed = true,
        }
    }

    /// Append one byte, bounds-checked.
    pub(crate) fn push_byte(&mut self, b: u8) {
        match self.buf.get_mut(self.pos) {
            Some(dst) => {
                *dst = b;
                self.pos += 1;
            }
            None => self.overflowed = true,
        }
    }
}

impl JsonSink for WriteSink<'_> {
    fn str(&mut self, s: &str) {
        self.push_bytes(s.as_bytes());
    }

    fn raw_int(&mut self, i: i64) {
        if i < 0 {
            self.push_byte(b'-');
        }
        let mut scratch = [0u8; MAX_U64_DIGITS];
        let digits = write_digits(i.unsigned_abs(), &mut scratch);
        self.push_bytes(digits);
    }

    fn fixed(&mut self, scaled: i64, frac_digits: u8, invert_sign: bool) {
        let (negative, magnitude) = fixed_sign_and_magnitude(scaled, invert_sign);
        if negative {
            self.push_byte(b'-');
        }
        let scale = pow10(frac_digits);
        let int_part = magnitude / scale;
        let frac_part = magnitude % scale;

        let mut scratch = [0u8; MAX_U64_DIGITS];
        let int_digits = write_digits(int_part, &mut scratch);
        self.push_bytes(int_digits);

        if frac_digits > 0 {
            self.push_byte(b'.');
            // Zero-pad the fraction to exactly `frac_digits` wide: write `frac_part`'s digits into
            // a fixed-width field, left-padding with '0' (e.g. frac_digits=3, frac_part=5 -> "005").
            let mut frac_scratch = [0u8; MAX_U64_DIGITS];
            let frac_digits_written = write_digits(frac_part, &mut frac_scratch);
            let pad = frac_digits as usize - frac_digits_written.len();
            for _ in 0..pad {
                self.push_byte(b'0');
            }
            self.push_bytes(frac_digits_written);
        }
    }

    fn null(&mut self) {
        self.push_bytes(b"null");
    }

    fn f32(&mut self, v: f32) {
        if v.is_finite() {
            f32_ryu(v, |s| self.push_bytes(s.as_bytes()));
        } else {
            self.push_bytes(b"null");
        }
    }

    fn f64(&mut self, v: f64) {
        if v.is_finite() {
            f64_ryu(v, |s| self.push_bytes(s.as_bytes()));
        } else {
            self.push_bytes(b"null");
        }
    }
}
