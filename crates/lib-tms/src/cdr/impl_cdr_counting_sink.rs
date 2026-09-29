//! `impl CdrSink for CdrCountingSink` + constructors — see `types/cdr_counting_sink.rs`'s doc
//! comment. Every method mirrors [`super::CdrWriteSink`]'s byte-count contribution EXACTLY (same
//! alignment, same widths) so a length counted here equals the bytes the write sink emits.

use super::align_up;
use super::prelude::*;

impl CdrCountingSink {
    /// A fresh counter at phase 0 (`pos == 0`). This is what [`crate::cdr::emit_dheader`] uses: a
    /// DHEADER-delimited subtree always begins at a 4-byte-aligned offset (the DHEADER is a 4-byte
    /// `u32`), so counting its contents from phase 0 yields exactly the bytes the write sink emits.
    pub fn new() -> Self {
        CdrCountingSink { pos: 0 }
    }

    /// A counter seeded so that `pos % 4 == phase % 4` — used by the alignment-phase property test
    /// to confirm a subtree's serialized size is position-independent (see
    /// `tests/cdr_primitives.rs`).
    pub fn at_phase(phase: usize) -> Self {
        CdrCountingSink { pos: phase % 4 }
    }

    /// The accumulated byte count.
    pub fn len(&self) -> usize {
        self.pos
    }

    /// True if nothing has been counted yet.
    pub fn is_empty(&self) -> bool {
        self.pos == 0
    }
}

impl CdrSink for CdrCountingSink {
    fn pos(&self) -> usize {
        self.pos
    }

    fn align(&mut self, n: usize) {
        self.pos = align_up(self.pos, n);
    }

    fn bytes(&mut self, b: &[u8]) {
        self.pos = self.pos.saturating_add(b.len());
    }

    fn u8(&mut self, _v: u8) {
        self.pos = self.pos.saturating_add(1);
    }

    fn u16(&mut self, _v: u16) {
        self.align(2);
        self.pos = self.pos.saturating_add(2);
    }

    fn u32(&mut self, _v: u32) {
        self.align(4);
        self.pos = self.pos.saturating_add(4);
    }

    fn u64(&mut self, _v: u64) {
        self.align(4);
        self.pos = self.pos.saturating_add(8);
    }

    fn i16(&mut self, _v: i16) {
        self.align(2);
        self.pos = self.pos.saturating_add(2);
    }

    fn i32(&mut self, _v: i32) {
        self.align(4);
        self.pos = self.pos.saturating_add(4);
    }

    fn f32(&mut self, _v: f32) {
        self.align(4);
        self.pos = self.pos.saturating_add(4);
    }

    fn f64(&mut self, _v: f64) {
        self.align(4);
        self.pos = self.pos.saturating_add(8);
    }

    fn boolean(&mut self, _v: bool) {
        self.pos = self.pos.saturating_add(1);
    }

    fn string(&mut self, s: &str) {
        self.u32(0); // length prefix (self-aligns to 4)
        self.pos = self.pos.saturating_add(s.len()).saturating_add(1); // body + NUL
    }

    fn enum_u32(&mut self, _v: u32) {
        self.u32(0);
    }
}
