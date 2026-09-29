//! [`ByteCursor`] — a bounds-checked, little-endian write cursor over a caller-owned `&mut [u8]`.
//!
//! `core`-only (no `alloc`) so it links on `thumbv8m.main-none-eabihf`. RTPS/CDR integers are
//! little-endian on the wire when the submessage EndiannessFlag is set (DDSI-RTPS 2.4 §9.4.5.1.1),
//! matching dust_dds's `to_le_bytes` convention.

/// A forward-only write cursor over a mutable byte buffer, tracking the current write position.
/// All multi-byte writes are little-endian. Panics (halt on the MCU) on overrun — callers size the
/// backing buffer with generous headroom (`SPDP_PKT_LEN`), so an overrun is a programming error, not
/// a runtime condition to recover from.
pub(crate) struct ByteCursor<'a> {
    pub(crate) buf: &'a mut [u8],
    pub(crate) pos: usize,
}
