//! `impl CdrReader` — the XCDR2 little-endian read cursor. See `types/cdr_reader.rs`. Each method
//! is the exact inverse of the matching [`super::CdrWriteSink`] write (same alignment, same width,
//! same NUL-inclusive string rule), so a decode reverses an encode byte-for-byte.

use super::align_up;
use super::prelude::*;

impl<'a> CdrReader<'a> {
    /// Wrap `buf` for reading from the start.
    pub fn new(buf: &'a [u8]) -> Self {
        CdrReader { buf, pos: 0 }
    }

    /// Current read offset (bytes from the start of the buffer).
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Advance past alignment padding to the next multiple of `min(n, 4)` (mirrors
    /// [`super::CdrSink::align`]).
    pub fn align(&mut self, n: usize) {
        self.pos = align_up(self.pos, n);
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], CdrError> {
        let end = self.pos.checked_add(len).ok_or(CdrError::Truncated)?;
        let slice = self.buf.get(self.pos..end).ok_or(CdrError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    /// Read a `u8` / `octet` (align 1).
    pub fn u8(&mut self) -> Result<u8, CdrError> {
        Ok(self.take(1)?[0])
    }

    /// Read a `boolean`: `0x00` → false, `0x01` → true, anything else → [`CdrError::BadBool`].
    pub fn boolean(&mut self) -> Result<bool, CdrError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(CdrError::BadBool),
        }
    }

    /// Read a `u16` little-endian (align 2).
    pub fn u16(&mut self) -> Result<u16, CdrError> {
        self.align(2);
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    /// Read a `u32` / `unsigned long` little-endian (align 4).
    pub fn u32(&mut self) -> Result<u32, CdrError> {
        self.align(4);
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Read a `u64` little-endian (align **4** in XCDR2).
    pub fn u64(&mut self) -> Result<u64, CdrError> {
        self.align(4);
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    /// Read an `i16` / `short` little-endian (align 2).
    pub fn i16(&mut self) -> Result<i16, CdrError> {
        Ok(self.u16()? as i16)
    }

    /// Read an `i32` / `long` little-endian (align 4).
    pub fn i32(&mut self) -> Result<i32, CdrError> {
        Ok(self.u32()? as i32)
    }

    /// Read an `f32` little-endian (align 4).
    pub fn f32(&mut self) -> Result<f32, CdrError> {
        Ok(f32::from_bits(self.u32()?))
    }

    /// Read an `f64` little-endian (align **4** in XCDR2).
    pub fn f64(&mut self) -> Result<f64, CdrError> {
        Ok(f64::from_bits(self.u64()?))
    }

    /// Read an enum's 4-byte holder integer (default `@bit_bound(32)`); the caller maps it to a
    /// variant and returns [`CdrError::BadEnum`] for an unknown ordinal.
    pub fn enum_u32(&mut self) -> Result<u32, CdrError> {
        self.u32()
    }

    /// Read a DHEADER (`u32`, align 4) and validate that the declared subtree length fits in the
    /// remaining buffer. Returns the declared length (bytes that follow the DHEADER).
    pub fn dheader(&mut self) -> Result<u32, CdrError> {
        let len = self.u32()?;
        let end = self
            .pos
            .checked_add(len as usize)
            .ok_or(CdrError::BadLength)?;
        if end > self.buf.len() {
            return Err(CdrError::BadLength);
        }
        Ok(len)
    }

    /// Read a `string`: a `u32` length prefix INCLUDING the NUL terminator, then the body, then the
    /// NUL byte (DDS-XTypes 1.3 §7.4.3.5 rule (3)). Returns the body as a borrowed `&str` (no
    /// allocation). Rejects a zero length, a missing/misplaced NUL, or non-UTF-8 bytes.
    pub fn string(&mut self) -> Result<&'a str, CdrError> {
        let total = self.u32()? as usize;
        if total == 0 {
            return Err(CdrError::BadString); // length always includes the NUL, so >= 1
        }
        let raw = self.take(total)?;
        // Last byte must be the NUL terminator.
        match raw.split_last() {
            Some((0, body)) => core::str::from_utf8(body).map_err(|_| CdrError::BadString),
            _ => Err(CdrError::BadString),
        }
    }
}
