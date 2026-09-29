//! Shared MQTT 3.1.1 wire primitives — fixed-header byte plumbing, the Remaining Length varint,
//! and UTF-8 string fields (MQTT 3.1.1 section 1.5.3 / 2.2.3) — used by both [`crate::link::mqtt_client`]
//! (the owned client encoders) and [`crate::link::mqtt_server`] (the broker core's parser). Extracted so
//! neither re-invents the wire format; this module owns none of the packet *semantics*, only the
//! byte-level building blocks every packet type is built from.
//!
//! Pure, `no_std`, no-alloc: every function takes caller-owned buffers/slices and a cursor, exactly
//! the seam the rest of the owned network stack uses.

mod types;

pub use types::*;

// -- MQTT 3.1.1 §2.2.1 control packet types (high nibble of the fixed-header byte 1) -------------

/// CONNECT (client -> broker).
pub const PKT_CONNECT: u8 = 0x1;
/// CONNACK (broker -> client).
pub const PKT_CONNACK: u8 = 0x2;
/// PUBLISH (either direction).
pub const PKT_PUBLISH: u8 = 0x3;
/// SUBSCRIBE (client -> broker).
pub const PKT_SUBSCRIBE: u8 = 0x8;
/// SUBACK (broker -> client).
pub const PKT_SUBACK: u8 = 0x9;
// UNSUBSCRIBE (0xA) / UNSUBACK (0xB) are intentionally omitted: unsubscribe is out of scope for
// this minimal broker (see `mqtt_server` module docs). Add them here if/when it grows an
// UNSUBSCRIBE handler.
/// PINGREQ (client -> broker).
pub const PKT_PINGREQ: u8 = 0xC;
/// PINGRESP (broker -> client).
pub const PKT_PINGRESP: u8 = 0xD;
/// DISCONNECT (client -> broker).
pub const PKT_DISCONNECT: u8 = 0xE;

/// Write one byte at `*pos`, advancing it. Fails closed if `out` has no room left.
pub fn put_u8(out: &mut [u8], pos: &mut usize, byte: u8) -> Result<(), WireError> {
    let slot = out.get_mut(*pos).ok_or(WireError::BufferTooSmall)?;
    *slot = byte;
    *pos += 1;
    Ok(())
}

/// Write a big-endian `u16` at `*pos` (the MQTT wire length-prefix width).
pub fn put_u16(out: &mut [u8], pos: &mut usize, value: u16) -> Result<(), WireError> {
    for byte in value.to_be_bytes() {
        put_u8(out, pos, byte)?;
    }
    Ok(())
}

/// Copy `bytes` in at `*pos`, advancing it.
pub fn put_bytes(out: &mut [u8], pos: &mut usize, bytes: &[u8]) -> Result<(), WireError> {
    let end = pos
        .checked_add(bytes.len())
        .ok_or(WireError::BufferTooSmall)?;
    let slot = out.get_mut(*pos..end).ok_or(WireError::BufferTooSmall)?;
    slot.copy_from_slice(bytes);
    *pos = end;
    Ok(())
}

/// Write an MQTT "UTF-8 encoded string" field: a 2-byte big-endian length prefix followed by the
/// raw bytes (MQTT 3.1.1 section 1.5.3). Fails closed if `bytes` exceeds the 16-bit length limit.
pub fn put_str_field(out: &mut [u8], pos: &mut usize, bytes: &[u8]) -> Result<(), WireError> {
    if bytes.len() > u16::MAX as usize {
        return Err(WireError::FieldTooLong);
    }
    put_u16(out, pos, bytes.len() as u16)?;
    put_bytes(out, pos, bytes)
}

/// Write the MQTT "Remaining Length" variable-length encoding (MQTT 3.1.1 section 2.2.3): 7 bits
/// per byte, continuation bit (0x80) set on all but the last byte.
pub fn put_remaining_length(
    out: &mut [u8],
    pos: &mut usize,
    mut len: usize,
) -> Result<(), WireError> {
    loop {
        let mut byte = (len % 128) as u8;
        len /= 128;
        if len > 0 {
            byte |= 0x80;
        }
        put_u8(out, pos, byte)?;
        if len == 0 {
            break;
        }
    }
    Ok(())
}

/// Read one byte at `*pos`, advancing it. Reports [`WireError::Truncated`] (not `Malformed`) if
/// `bytes` is exhausted — the caller may just not have received the rest of the packet yet.
pub fn get_u8(bytes: &[u8], pos: &mut usize) -> Result<u8, WireError> {
    let b = *bytes.get(*pos).ok_or(WireError::Truncated)?;
    *pos += 1;
    Ok(b)
}

/// Read a big-endian `u16` at `*pos`, advancing it.
pub fn get_u16(bytes: &[u8], pos: &mut usize) -> Result<u16, WireError> {
    let hi = get_u8(bytes, pos)?;
    let lo = get_u8(bytes, pos)?;
    Ok(u16::from_be_bytes([hi, lo]))
}

/// Read `len` raw bytes at `*pos`, advancing it, returning the slice. Reports
/// [`WireError::Truncated`] if `bytes` doesn't yet hold `len` more bytes (an oversized `len`
/// causing `usize` overflow is the one case treated as genuinely [`WireError::Malformed`]).
pub fn get_bytes<'a>(bytes: &'a [u8], pos: &mut usize, len: usize) -> Result<&'a [u8], WireError> {
    let end = pos.checked_add(len).ok_or(WireError::Malformed)?;
    let slice = bytes.get(*pos..end).ok_or(WireError::Truncated)?;
    *pos = end;
    Ok(slice)
}

/// Read an MQTT "UTF-8 encoded string" field (2-byte big-endian length prefix + raw bytes),
/// advancing `*pos`, and validate it as UTF-8 (MQTT 3.1.1 section 1.5.3).
pub fn get_str_field<'a>(bytes: &'a [u8], pos: &mut usize) -> Result<&'a str, WireError> {
    let len = get_u16(bytes, pos)? as usize;
    let raw = get_bytes(bytes, pos, len)?;
    core::str::from_utf8(raw).map_err(|_| WireError::Malformed)
}

/// Decode the MQTT "Remaining Length" variable-length field (MQTT 3.1.1 section 2.2.3) starting at
/// `*pos`, advancing it past the varint. Rejects encodings longer than the spec's 4-byte maximum
/// (a malformed/hostile packet), never looping unbounded.
pub fn get_remaining_length(bytes: &[u8], pos: &mut usize) -> Result<usize, WireError> {
    let mut value: usize = 0;
    let mut multiplier: usize = 1;
    for _ in 0..4 {
        let byte = get_u8(bytes, pos)?;
        value = value
            .checked_add((byte as usize & 0x7F) * multiplier)
            .ok_or(WireError::Malformed)?;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        multiplier = multiplier.checked_mul(128).ok_or(WireError::Malformed)?;
    }
    // A 5th continuation byte would exceed the spec's 4-byte maximum encoding.
    Err(WireError::Malformed)
}

/// Parse the fixed header (MQTT 3.1.1 section 2.2) at the start of `bytes`. Returns
/// [`WireError::Truncated`] if there are not yet enough bytes buffered for a complete header — the
/// caller should wait for more input, not treat that as a protocol violation. Returns
/// [`WireError::Malformed`] only for a genuine violation (a Remaining Length encoding that already
/// exceeds the spec's 4-byte maximum with the bytes in hand). Note this only validates the fixed
/// header itself; the caller must still check `header_len + remaining_length <= bytes.len()` before
/// trusting the *whole* packet (header + variable header/payload) is complete.
pub fn parse_fixed_header(bytes: &[u8]) -> Result<FixedHeader, WireError> {
    let mut pos = 0usize;
    let first = get_u8(bytes, &mut pos)?;
    let remaining_length = get_remaining_length(bytes, &mut pos)?;
    Ok(FixedHeader {
        packet_type: first >> 4,
        flags: first & 0x0F,
        remaining_length,
        header_len: pos,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn remaining_length_round_trips_boundary_values() {
        for &len in &[
            0usize,
            127,
            128,
            16383,
            16384,
            2_097_151,
            2_097_152,
            268_435_455,
        ] {
            let mut buf = [0u8; 4];
            let mut pos = 0usize;
            put_remaining_length(&mut buf, &mut pos, len).unwrap();
            let encoded_len = pos;
            let mut dpos = 0usize;
            let decoded = get_remaining_length(&buf[..encoded_len], &mut dpos).unwrap();
            assert_eq!(decoded, len);
            assert_eq!(dpos, encoded_len);
        }
    }

    #[test]
    fn remaining_length_rejects_five_continuation_bytes() {
        // 5 bytes, all with the continuation bit set — exceeds the spec's 4-byte maximum. This is
        // a genuine protocol violation (Malformed), not "need more input" (Truncated): the 4 bytes
        // already in hand are all continuation-flagged, so no completion of the varint is valid.
        let buf = [0x80, 0x80, 0x80, 0x80, 0x01];
        let mut pos = 0usize;
        assert_eq!(
            get_remaining_length(&buf, &mut pos),
            Err(WireError::Malformed)
        );
    }

    #[test]
    fn remaining_length_reports_truncated_when_bytes_run_out_mid_varint() {
        // A single continuation-flagged byte with nothing after it: this *could* still become a
        // valid (<=4-byte) varint once more bytes arrive, so it must be Truncated, not Malformed —
        // callers reassembling a byte stream (e.g. mqtt_server::BrokerCore::feed_incoming) rely on
        // this distinction to know whether to wait for more input or close the connection.
        let buf = [0x80];
        let mut pos = 0usize;
        assert_eq!(
            get_remaining_length(&buf, &mut pos),
            Err(WireError::Truncated)
        );
    }

    #[test]
    fn fixed_header_reports_truncated_on_a_bare_type_byte() {
        // Only the type/flags byte buffered so far, no Remaining Length byte(s) yet.
        let buf = [0x10];
        assert_eq!(parse_fixed_header(&buf), Err(WireError::Truncated));
    }

    #[test]
    fn str_field_round_trips() {
        let mut buf = [0u8; 32];
        let mut pos = 0usize;
        put_str_field(&mut buf, &mut pos, b"hello").unwrap();
        let mut dpos = 0usize;
        let s = get_str_field(&buf[..pos], &mut dpos).unwrap();
        assert_eq!(s, "hello");
        assert_eq!(dpos, pos);
    }

    #[test]
    fn str_field_rejects_invalid_utf8() {
        // length prefix 1, then an invalid UTF-8 byte.
        let buf = [0x00, 0x01, 0xFF];
        let mut pos = 0usize;
        assert_eq!(get_str_field(&buf, &mut pos), Err(WireError::Malformed));
    }

    #[test]
    fn fixed_header_reports_type_flags_and_length() {
        // PUBLISH, QoS0, remaining length 7 (from mqtt_session's PUBLISH_QOS0_VECTOR).
        let buf = [0x30, 0x07];
        let h = parse_fixed_header(&buf).unwrap();
        assert_eq!(h.packet_type, PKT_PUBLISH);
        assert_eq!(h.flags, 0);
        assert_eq!(h.remaining_length, 7);
        assert_eq!(h.header_len, 2);
    }
}
