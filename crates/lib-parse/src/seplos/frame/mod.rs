//! Seplos ASCII request/response framing.
//!
//! Wire layout (all fields ASCII-hex unless noted):
//! * Request:  `~ VER(20) ADR(2) CID1(46) CID2(2) LENGTH(4) INFO(*) CHKSUM(4) \r`
//! * Response: `~ VER(20) ADR(2) CID1(46) RTN(2)  LENGTH(4) INFO(*) CHKSUM(4) \r`
//!
//! SOI = `~` (0x7E), EOI = `\r` (0x0D). Header before INFO = 12 chars; CHKSUM = 4 chars;
//! minimum response length (EOI stripped) = 17. The response's 5th field is **RTN**, not a
//! CID2 echo — the decoder is keyed by the command actually sent (the poll cadence lives in
//! `impl-app::run`).
//!
//! CONTRACT ONLY — bodies are pipeline FILL targets.

mod prelude;
mod types;

mod impl_response_frame;

pub use types::*;

/// Start-of-info byte.
pub const SOI: u8 = b'~';
/// End-of-info byte.
pub const EOI: u8 = b'\r';
/// Protocol version field value (ASCII "20").
pub const VER: &[u8; 2] = b"20";
/// CID1 field value (ASCII "46").
pub const CID1: &[u8; 2] = b"46";
