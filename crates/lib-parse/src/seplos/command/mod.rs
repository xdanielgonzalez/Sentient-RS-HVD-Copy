//! Seplos command-frame builder — the single authoritative builder (the prototype had two
//! divergent implementations; this is the one contract).
//!
//! Builds `~ VER ADR CID1 CID2 LENGTH INFO CHKSUM \r` for a given DIP address and command.
//! For `0x42`/`0x44`, INFO = one ASCII-hex digit of the DIP address, LENID = `001`. For `0xA4`
//! the LENID/INFO convention is resolved against real device captures (see plan risk note);
//! the default follows the poller variant. DIP address encodes 0..=15 as `'0'..='9','A'..='F'`.
//!
//! CONTRACT ONLY — bodies are pipeline FILL targets.

mod impl_command;
mod prelude;
mod types;

use prelude::*;

pub use types::*;

/// Build a full command frame for `dip` (0..=15) and `command` into `out`, returning the number
/// of bytes written. Returns [`SeplosError::AddrOutOfRange`] if `dip > 15`.
pub fn build_command(
    dip: u8,
    command: Command,
    out: &mut heapless::Vec<u8, 32>,
) -> Result<(), SeplosError> {
    if dip > 15 {
        return Err(SeplosError::AddrOutOfRange(dip));
    }

    out.clear();

    // Helper to convert a nibble (0..=15) to a single ASCII hex digit.
    let nibble_to_hex = |n: u8| -> u8 {
        if n < 10 {
            b'0' + n
        } else {
            b'A' + (n - 10)
        }
    };

    // Push '~'
    out.push(b'~').map_err(|_| SeplosError::TooManyElements)?;

    // Push VER "20"
    out.push(b'2').map_err(|_| SeplosError::TooManyElements)?;
    out.push(b'0').map_err(|_| SeplosError::TooManyElements)?;

    // Push ADR (2 hex digits of dip)
    out.push(nibble_to_hex((dip >> 4) & 0xF))
        .map_err(|_| SeplosError::TooManyElements)?;
    out.push(nibble_to_hex(dip & 0xF))
        .map_err(|_| SeplosError::TooManyElements)?;

    // Push CID1 "46"
    out.push(b'4').map_err(|_| SeplosError::TooManyElements)?;
    out.push(b'6').map_err(|_| SeplosError::TooManyElements)?;

    // Push CID2
    let cid2_bytes = command.cid2();
    out.push(cid2_bytes[0])
        .map_err(|_| SeplosError::TooManyElements)?;
    out.push(cid2_bytes[1])
        .map_err(|_| SeplosError::TooManyElements)?;

    // Push LENGTH (4 hex digits)
    let length_val = super::checksum::length_field(0x001);
    out.push(nibble_to_hex(((length_val >> 12) & 0xF) as u8))
        .map_err(|_| SeplosError::TooManyElements)?;
    out.push(nibble_to_hex(((length_val >> 8) & 0xF) as u8))
        .map_err(|_| SeplosError::TooManyElements)?;
    out.push(nibble_to_hex(((length_val >> 4) & 0xF) as u8))
        .map_err(|_| SeplosError::TooManyElements)?;
    out.push(nibble_to_hex((length_val & 0xF) as u8))
        .map_err(|_| SeplosError::TooManyElements)?;

    // Push INFO (1 hex digit of dip)
    out.push(nibble_to_hex(dip & 0xF))
        .map_err(|_| SeplosError::TooManyElements)?;

    // Calculate CHKSUM over the ASCII bytes from position 1 onward (VER+ADR+CID1+CID2+LENGTH+INFO)
    let checksum = super::checksum::frame_checksum(&out[1..]);

    // Push CHKSUM (4 hex digits)
    out.push(nibble_to_hex(((checksum >> 12) & 0xF) as u8))
        .map_err(|_| SeplosError::TooManyElements)?;
    out.push(nibble_to_hex(((checksum >> 8) & 0xF) as u8))
        .map_err(|_| SeplosError::TooManyElements)?;
    out.push(nibble_to_hex(((checksum >> 4) & 0xF) as u8))
        .map_err(|_| SeplosError::TooManyElements)?;
    out.push(nibble_to_hex((checksum & 0xF) as u8))
        .map_err(|_| SeplosError::TooManyElements)?;

    // Push '\r'
    out.push(b'\r').map_err(|_| SeplosError::TooManyElements)?;

    Ok(())
}
