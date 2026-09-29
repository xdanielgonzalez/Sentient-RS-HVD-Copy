//! `impl CdrSink for CdrWriteSink` + inherent constructors/finalizer — see
//! `types/cdr_write_sink.rs`'s doc comment for the overflow-latch contract.

use super::align_up;
use super::prelude::*;

impl<'a> CdrWriteSink<'a> {
    /// Wrap `buf` for writing from the start.
    pub fn new(buf: &'a mut [u8]) -> Self {
        CdrWriteSink {
            buf,
            pos: 0,
            overflowed: false,
        }
    }

    /// Bytes written so far (only meaningful if [`CdrWriteSink::overflowed`] is `false`).
    pub fn len(&self) -> usize {
        self.pos
    }

    /// True if nothing has been written yet.
    pub fn is_empty(&self) -> bool {
        self.pos == 0
    }

    /// True if any write since construction was skipped because `buf` ran out of room. The driving
    /// encoder must check this and treat the buffer as invalid rather than trusting [`len`].
    ///
    /// [`len`]: CdrWriteSink::len
    pub fn overflowed(&self) -> bool {
        self.overflowed
    }

    /// Append raw bytes, bounds-checked; sets [`CdrWriteSink::overflowed`] and writes nothing if
    /// `b` would not fit in the remaining capacity (never a partial write).
    pub(crate) fn push_bytes(&mut self, b: &[u8]) {
        let end = self.pos.saturating_add(b.len());
        match self.buf.get_mut(self.pos..end) {
            Some(dst) => {
                dst.copy_from_slice(b);
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
                self.pos = self.pos.saturating_add(1);
            }
            None => self.overflowed = true,
        }
    }

    /// Finalize an RTPS SerializedPayload written via [`crate::cdr::emit_object_cdr`]: pad the
    /// payload with zero bytes up to the next 4-byte boundary and record the padding count in the
    /// low two bits of the second options byte (byte index 3), per DDS-XTypes 1.3 §7.6.3.1.2 ("the
    /// least significant two bits shall be set to … the number of padding bytes needed after the end
    /// of the serialized payload in order to reach the next 4-byte aligned offset"). Assumes the
    /// first 4 bytes are the encapsulation identifier + options header.
    pub fn finalize_encapsulation_padding(&mut self) {
        // Payload length excludes the 4-byte encapsulation header.
        let payload_len = self.pos.saturating_sub(4);
        let pad = (4 - (payload_len % 4)) % 4;
        if let Some(opt) = self.buf.get_mut(3) {
            *opt |= pad as u8;
        }
        for _ in 0..pad {
            self.push_byte(0);
        }
    }
}

impl CdrSink for CdrWriteSink<'_> {
    fn pos(&self) -> usize {
        self.pos
    }

    fn align(&mut self, n: usize) {
        let target = align_up(self.pos, n);
        for _ in self.pos..target {
            self.push_byte(0);
        }
    }

    fn bytes(&mut self, b: &[u8]) {
        self.push_bytes(b);
    }

    fn u8(&mut self, v: u8) {
        self.push_byte(v);
    }

    fn u16(&mut self, v: u16) {
        self.align(2);
        self.push_bytes(&v.to_le_bytes());
    }

    fn u32(&mut self, v: u32) {
        self.align(4);
        self.push_bytes(&v.to_le_bytes());
    }

    fn u64(&mut self, v: u64) {
        self.align(4); // XCDR2: 8-byte primitives align to 4 (DDS-XTypes 1.3 §7.4.2)
        self.push_bytes(&v.to_le_bytes());
    }

    fn i16(&mut self, v: i16) {
        self.align(2);
        self.push_bytes(&v.to_le_bytes());
    }

    fn i32(&mut self, v: i32) {
        self.align(4);
        self.push_bytes(&v.to_le_bytes());
    }

    fn f32(&mut self, v: f32) {
        self.align(4);
        self.push_bytes(&v.to_le_bytes());
    }

    fn f64(&mut self, v: f64) {
        self.align(4); // XCDR2: FLOAT64 aligns to 4 (DDS-XTypes 1.3 §7.4.2)
        self.push_bytes(&v.to_le_bytes());
    }

    fn boolean(&mut self, v: bool) {
        self.push_byte(u8::from(v));
    }

    fn string(&mut self, s: &str) {
        let body = s.as_bytes();
        // Length prefix INCLUDES the NUL terminator (DDS-XTypes 1.3 §7.4.3.5 rule (3)).
        self.u32((body.len() as u32).saturating_add(1));
        self.push_bytes(body);
        self.push_byte(0);
    }

    fn enum_u32(&mut self, v: u32) {
        self.u32(v);
    }
}
