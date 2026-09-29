//! [`CdrReader`] — the XCDR2 decode cursor. See `impl_cdr_reader.rs` for the read methods and
//! `crate::cdr::decode` for the per-type decoders that drive it.

/// A little-endian XCDR2 read cursor over a borrowed byte buffer. Tracks the current `pos` so that
/// alignment (`pos % 4`, origin 0) mirrors exactly what [`super::CdrWriteSink`] produced on the
/// write side. Every read is bounds-checked and returns [`super::CdrError`] on truncation rather
/// than panicking (the crate is `#![forbid(unsafe_code)]`).
///
/// This decode path is intentionally minimal: it targets the two inbound TMS request types
/// (`EnergyStartStopRequest`, `PowerSwitchRequest`) the device must accept. The full generated
/// decode surface is out of scope for this crate (the device *renders* on output and only *reads*
/// requests).
pub struct CdrReader<'a> {
    pub(crate) buf: &'a [u8],
    pub(crate) pos: usize,
}
