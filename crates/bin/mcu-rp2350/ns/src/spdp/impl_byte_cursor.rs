//! Behavior for [`ByteCursor`]: the little-endian primitive writers plus the two RTPS/CDR framing
//! helpers ([`param_header`](ByteCursor::param_header) and [`locator`](ByteCursor::locator)) the SPDP
//! encoder builds on. See `types/byte_cursor.rs` for the type and the endianness rationale.

use super::types::ByteCursor;

impl<'a> ByteCursor<'a> {
    /// Wrap a mutable buffer, starting the write position at 0.
    #[inline]
    pub(crate) fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// The current write position, i.e. the total number of bytes written so far.
    #[inline]
    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    /// Append a raw byte slice verbatim (no alignment, no length prefix).
    #[inline]
    pub(crate) fn bytes(&mut self, b: &[u8]) {
        self.buf[self.pos..self.pos + b.len()].copy_from_slice(b);
        self.pos += b.len();
    }

    /// Append a single byte (e.g. a submessageId or flags octet).
    #[inline]
    pub(crate) fn u8(&mut self, v: u8) {
        self.buf[self.pos] = v;
        self.pos += 1;
    }

    /// Append a little-endian `u16` (submessage lengths, parameter ids/lengths).
    #[inline]
    pub(crate) fn u16_le(&mut self, v: u16) {
        self.bytes(&v.to_le_bytes());
    }

    /// Append a little-endian `u32` (locator ports, the builtin-endpoint bitset, domain id).
    #[inline]
    pub(crate) fn u32_le(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }

    /// Append a little-endian `i32` (locator `kind`, `Duration` seconds — both signed `Long` in
    /// DDSI-RTPS 2.4).
    #[inline]
    pub(crate) fn i32_le(&mut self, v: i32) {
        self.bytes(&v.to_le_bytes());
    }

    /// Overwrite two bytes at an earlier position with a little-endian `u16` — used to back-patch the
    /// DATA submessage's `submessageLength` once the body length is known.
    #[inline]
    pub(crate) fn patch_u16_le(&mut self, at: usize, v: u16) {
        self.buf[at..at + 2].copy_from_slice(&v.to_le_bytes());
    }

    /// Emit a PL_CDR parameter header: `parameterId` (u16 LE) then `length` (u16 LE), the length
    /// being the byte count of the value that follows (which the CDR rules require be a multiple of
    /// 4 — DDSI-RTPS 2.4 §9.4.2.11 ParameterList). Every value this encoder writes is already a
    /// 4-multiple, so no trailing pad is ever needed after the value.
    #[inline]
    pub(crate) fn param_header(&mut self, pid: u16, len: u16) {
        self.u16_le(pid);
        self.u16_le(len);
    }

    /// Emit a 24-byte RTPS `Locator_t` value body (DDSI-RTPS 2.4 §8.3.5.5, layout confirmed against
    /// `vendor/dust_dds/src/transport/types.rs:226-230`): `kind` (i32 LE) ++ `port` (u32 LE) ++
    /// `address` (16 bytes, network order). Caller writes the enclosing [`param_header`] with
    /// `len = 24` first.
    #[inline]
    pub(crate) fn locator(&mut self, kind: i32, port: u32, address: &[u8; 16]) {
        self.i32_le(kind);
        self.u32_le(port);
        self.bytes(address);
    }
}
