use super::prelude::*;

/// Lowercase-hex-encode `bytes` into `out` (`out.len() == bytes.len() * 2`, enforced by callers —
/// both call sites here size `out` exactly).
pub(super) fn encode_hex(bytes: &[u8], out: &mut [u8]) {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    for (i, byte) in bytes.iter().enumerate() {
        out[i * 2] = DIGITS[usize::from(byte >> 4)];
        out[i * 2 + 1] = DIGITS[usize::from(byte & 0x0F)];
    }
}

/// Decode a hex string into `out` (`out.len()` raw bytes, so `hex.len()` must be `out.len() * 2`).
/// Any non-hex-digit byte, or a wrong-length input, is [`SecurityError::Malformed`] (fail closed —
/// this only ever handles caller-supplied token bytes).
pub(super) fn decode_hex(hex: &[u8], out: &mut [u8]) -> Result<()> {
    if hex.len() != out.len() * 2 {
        return Err(SecurityError::Malformed);
    }
    for (i, slot) in out.iter_mut().enumerate() {
        let hi = hex_nibble(hex[i * 2])?;
        let lo = hex_nibble(hex[i * 2 + 1])?;
        *slot = (hi << 4) | lo;
    }
    Ok(())
}

/// A single hex digit (`0-9a-fA-F`) to its 4-bit value; anything else is
/// [`SecurityError::Malformed`].
fn hex_nibble(c: u8) -> Result<u8> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(SecurityError::Malformed),
    }
}
