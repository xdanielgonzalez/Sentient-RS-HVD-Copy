//! Seplos frame checksums.
//!
//! Two independent checks:
//! * **Frame CHKSUM** — 16-bit two's-complement negate of the ASCII byte-sum of `VER..INFO`.
//!   `sum = Σ ascii_bytes`; `cs = ((!(sum & 0xFFFF)) + 1) & 0xFFFF`. The prototype omitted the
//!   final `& 0xFFFF` mask after the `+1`, producing a 5-char `"10000"` overflow when
//!   `sum % 0x10000 == 0`. We mask; that bug is not inherited.
//! * **LENGTH LCHKSUM** — a nibble check over the 3 hex chars of LENID:
//!   `lchksum = ((0xF - (Σ nibble_values) % 16) + 1) & 0xF`.
//!
//! CONTRACT ONLY — the algorithm above is the authoritative spec (planner output). The bodies
//! below are FILL targets produced by the enforced planner→scaffold→Haiku-fill→gate pipeline.
//!
//! # Example
//!
//! ```rust
//! use lib_parse::seplos::checksum::{frame_checksum, length_field};
//!
//! // The DIP-0 telemetry request body `VER..INFO` sums to CHKSUM 0xFD67 — the trailing
//! // `FD67` in the on-wire frame `~20004642F0010FD67\r`.
//! assert_eq!(frame_checksum(b"20004642F0010"), 0xFD67);
//!
//! // LENGTH for LENID 0x001 (one INFO hex char): LCHKSUM nibble 0xF in the high nibble.
//! assert_eq!(length_field(0x001), 0xF001);
//! ```

/// Compute the frame CHKSUM over the ASCII bytes of `VER + ADR + CID1 + CID2 + LENGTH + INFO`
/// (everything between SOI and the CHKSUM field, exclusive).
pub fn frame_checksum(body_ascii: &[u8]) -> u16 {
    let sum: u32 = body_ascii.iter().fold(0u32, |acc, &byte| acc + byte as u32);
    let low = (sum & 0xFFFF) as u16;
    (!low).wrapping_add(1)
}

/// Compute the LCHKSUM nibble for a given LENID (the low 12 bits / 3 hex chars of LENGTH).
pub fn length_checksum(lenid: u16) -> u8 {
    let n0 = (lenid >> 8) & 0xF;
    let n1 = (lenid >> 4) & 0xF;
    let n2 = lenid & 0xF;
    let s = ((n0 + n1 + n2) % 16) as u8;
    ((0xF - s) + 1) & 0xF
}

/// Assemble the full 16-bit LENGTH field: high nibble = LCHKSUM, low 12 bits = LENID.
pub fn length_field(lenid: u16) -> u16 {
    ((length_checksum(lenid) as u16) << 12) | (lenid & 0x0FFF)
}
