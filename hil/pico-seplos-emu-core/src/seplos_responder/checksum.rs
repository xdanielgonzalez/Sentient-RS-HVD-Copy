//! Seplos frame checksums — VENDORED VERBATIM (byte-identical) from
//! `crates/lib-parse/src/seplos/checksum.rs`, the single authoritative implementation. Not a
//! dependency on `lib-parse` because that crate's `Cargo.toml` carries an unconditional (not
//! feature-gated) `bytes` dependency whose atomic ref-counting needs RMW instructions
//! `thumbv6m-none-eabi` (ARMv6-M / Cortex-M0+, no LDREX/STREX) doesn't have — verified by
//! `cargo check -p lib-parse --no-default-features --features seplos --target
//! thumbv6m-none-eabi` failing on `bytes`'s `AtomicUsize::fetch_add`/`compare_exchange`, and
//! `bytes` itself is dead code in `lib-parse` (`grep -rn "bytes::" crates/lib-parse/src` = no
//! hits). If `lib-parse` ever drops/feature-gates `bytes`, this file should be replaced by a real
//! `lib-parse` dependency — do not let it silently drift from the original.
//!
//! Two independent checks:
//! * **Frame CHKSUM** — 16-bit two's-complement negate of the ASCII byte-sum of `VER..INFO`.
//!   `sum = Σ ascii_bytes`; `cs = ((!(sum & 0xFFFF)) + 1) & 0xFFFF`.
//! * **LENGTH LCHKSUM** — a nibble check over the 3 hex chars of LENID:
//!   `lchksum = ((0xF - (Σ nibble_values) % 16) + 1) & 0xF`.
//!
//! # Example
//!
//! `checksum` is `pub(crate)` (an internal implementation detail, not part of this crate's public
//! API — see the vendoring note above), so this example is illustrative only, not a runnable
//! doctest; the equivalent assertions run for real as `#[test]`s at the bottom of this file.
//!
//! ```ignore
//! use pico_seplos_emu_core::seplos_responder::checksum::{frame_checksum, length_field};
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_checksum_matches_the_dip0_telemetry_worked_example() {
        assert_eq!(frame_checksum(b"20004642F0010"), 0xFD67);
    }

    #[test]
    fn length_field_matches_the_lenid_1_worked_example() {
        assert_eq!(length_field(0x001), 0xF001);
    }
}
