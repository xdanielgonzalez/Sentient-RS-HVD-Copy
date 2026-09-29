//! Hand-rolled Meshtastic on-air protobuf + stream framing for the single message shape this
//! crate needs: `ToRadio{ packet: MeshPacket{ to, channel, decoded: Data{ portnum, payload } } }`.
//!
//! **Why hand-rolled, not `prost`/the `meshtastic` crate** (supply-chain decision 0006): pulling
//! in a full protobuf codegen toolchain, or the `meshtastic` crate's entire generated message
//! graph, for four scalar fields is a disproportionate dependency footprint. **Sharper reason,
//! found on re-verification (2026-07-25):** both the `meshtastic` crate and `meshtastic/protobufs`
//! are **GPL-3.0** — generating from the upstream `.proto` sources (even via `prost`/`micropb`,
//! themselves permissively licensed) would still consume GPL-3.0-licensed source, a copyleft
//! ingestion event for this proprietary binary. The wire format itself (protobuf varint +
//! length-delimited + fixed32 encoding) is a small, openly-specified binary format — a good fit
//! for a minimal hand-rolled encoder pinned by byte-level golden tests (see `tests` below).
//!
//! **Verified** against `meshtastic/protobufs` @ master and `meshtastic/firmware` @ master
//! (2026-07-25) — see the "Meshtastic Serial/UART Protobuf Client API (Heltec V4)" research note.
//! Field numbers: `ToRadio.packet = 1`, `MeshPacket.to = 2` (`fixed32`), `MeshPacket.channel = 3`,
//! `MeshPacket.decoded = 4`, `Data.portnum = 1`, `Data.payload = 2`, `PortNum.PRIVATE_APP = 256`.
//!
//! **#386 fix — `MeshPacket.to` must be set explicitly to `0xFFFFFFFF` (broadcast).** The previous
//! revision of this module bet on "proto3's absent-field-means-broadcast convention"; that
//! assumption is **falsified**. `meshtastic/firmware`'s `Router::sendLocal()` contains
//! `if (p->to == 0) { LOG_ERROR("Packet received with to: of 0!"); }`, and no firmware path
//! rewrites an absent/zero `to` to `NODENUM_BROADCAST`. A frame with no `to` field is therefore
//! very likely **never delivered** — this fix is safety-load-bearing (mesh is the out-of-band
//! channel that must survive a network outage). `to` is `fixed32` (wire type 5): tag `0x15`, then
//! four `0xFF` bytes (little-endian, though all four bytes are `0xFF` regardless of endianness).
//!
//! Stream framing (distinct from the protobuf encoding above): Meshtastic's serial `ToRadio`/
//! `FromRadio` framing is `0x94 0xC3` magic bytes, then a big-endian `u16` payload length, then the
//! raw protobuf message bytes.
//!
//! **No `alloc`: one caller-supplied `&mut [u8]`, reverse-encoded.** Nested length-delimited
//! protobuf needs the inner message's exact byte length before the outer tag+length can be
//! written — the classic "know the length before you've written it" problem. Rather than
//! encode inner-first into a scratch buffer (extra stack space per nesting level) or allocate
//! (ruled out for `no_std`/no-alloc entirely), every `write_*` function here writes BACKWARD: a
//! `cursor: &mut usize` starts at `out.len()` and decreases as bytes are placed, so by the time an
//! outer tag+length needs writing, the inner content's exact length is simply `old_cursor -
//! new_cursor` — no scratch buffer, no two-pass, no allocation, and the final encoded region is
//! always a single contiguous slice `out[cursor..]` that `to_radio_frame` moves into place with
//! one `copy_within` (a memmove, safe regardless of overlap direction).

use super::prelude::*;

/// `PortNum.PRIVATE_APP` — Meshtastic's reserved port range for custom, non-standard payloads.
/// `pub` (not `pub(crate)`): both the Pi's `run_radio` (`mesh-tokio`) and the MCU's own mesh sender
/// (WP3, a different crate) build a [`MeshFrame`] and need the same portnum constant rather than
/// each hardcoding the magic number `256` independently.
pub const PORTNUM_PRIVATE_APP: u32 = 256;

/// `NODENUM_BROADCAST` — the value `MeshPacket.to` must carry for every frame this crate sends
/// (this codec never sends anything but broadcast frames). See this module's doc comment for why
/// this is mandatory, not the proto3 zero-value default.
const NODENUM_BROADCAST: u32 = 0xFFFF_FFFF;

/// Meshtastic serial stream framing magic bytes, preceding every `ToRadio`/`FromRadio` protobuf
/// message on the wire.
const FRAME_MAGIC: [u8; 2] = [0x94, 0xC3];

/// Ceiling on the encoded on-air frame this crate will ever produce (stream header + protobuf
/// bytes) — the firmware's own `MAX_TO_FROM_RADIO_SIZE` (a declared length above this makes the
/// receiver discard and resync, per `StreamAPI.h`), generous headroom over the ~200-byte
/// condensation budget. Used to size the reusable send buffer.
pub const MAX_RADIO_FRAME: usize = 512;

/// Number of base-128 groups `write_varint` needs to encode `value`.
fn varint_len(value: u64) -> usize {
    let mut v = value;
    let mut len = 1;
    while v >= 0x80 {
        v >>= 7;
        len += 1;
    }
    len
}

/// Write `value` as a protobuf base-128 varint ending at `out[..*cursor]` (i.e. occupying
/// `out[*cursor - len..*cursor]` on success), decrementing `*cursor` by the bytes written. Returns
/// `false` (writing nothing) if `*cursor` bytes of room isn't enough.
fn write_varint(out: &mut [u8], cursor: &mut usize, value: u64) -> bool {
    let len = varint_len(value);
    if *cursor < len {
        return false;
    }
    let start = *cursor - len;
    let mut v = value;
    for i in 0..len {
        let more = i + 1 < len;
        out[start + i] = ((v & 0x7F) as u8) | if more { 0x80 } else { 0 };
        v >>= 7;
    }
    *cursor = start;
    true
}

/// Write a protobuf field tag `(field_number << 3) | wire_type`, backward (see module doc).
fn write_tag(out: &mut [u8], cursor: &mut usize, field_number: u32, wire_type: u8) -> bool {
    write_varint(
        out,
        cursor,
        (u64::from(field_number) << 3) | u64::from(wire_type),
    )
}

/// Write a varint-typed field (wire type 0) — OMITTED entirely when `value == 0` (proto3's
/// standard, lossless default-value wire-format optimization: a decoder treats an absent scalar
/// field as its zero/default value).
fn write_varint_field(out: &mut [u8], cursor: &mut usize, field_number: u32, value: u64) -> bool {
    if value == 0 {
        return true;
    }
    write_varint(out, cursor, value) && write_tag(out, cursor, field_number, 0)
}

/// Write a `fixed32`-typed field (wire type 5), unconditionally — no zero-value elision, since
/// this codec's only fixed32 field (`MeshPacket.to`) is always the meaningful, non-default
/// broadcast address (#386), never something a caller might want omitted.
fn write_fixed32_field(out: &mut [u8], cursor: &mut usize, field_number: u32, value: u32) -> bool {
    if *cursor < 4 {
        return false;
    }
    let start = *cursor - 4;
    out[start..*cursor].copy_from_slice(&value.to_le_bytes());
    *cursor = start;
    write_tag(out, cursor, field_number, 5)
}

/// Write a length-delimited field (wire type 2) whose body is exactly `bytes` — for a field
/// that's already a flat byte slice (`Data.payload`), as opposed to a nested submessage (see
/// [`write_len_delimited_nested`]).
fn write_len_delimited_bytes(
    out: &mut [u8],
    cursor: &mut usize,
    field_number: u32,
    bytes: &[u8],
) -> bool {
    write_len_delimited_nested(out, cursor, field_number, |out, cursor| {
        if *cursor < bytes.len() {
            return false;
        }
        let start = *cursor - bytes.len();
        out[start..*cursor].copy_from_slice(bytes);
        *cursor = start;
        true
    })
}

/// Write a length-delimited field (wire type 2) whose body is produced by `write_body` — the
/// nested-submessage primitive. `write_body` runs FIRST (writing backward, so it lands at the
/// higher addresses / later position), then this function measures exactly how much it wrote
/// (`before - *cursor`) and prepends the length varint and tag ahead of it. This is what makes
/// nested messages (`MeshPacket.decoded` wrapping `Data`, `ToRadio.packet` wrapping `MeshPacket`)
/// possible with zero scratch space: the "inner content" and "outer wrapper" share the same
/// buffer and cursor.
fn write_len_delimited_nested(
    out: &mut [u8],
    cursor: &mut usize,
    field_number: u32,
    write_body: impl FnOnce(&mut [u8], &mut usize) -> bool,
) -> bool {
    let before = *cursor;
    if !write_body(out, cursor) {
        return false;
    }
    let body_len = (before - *cursor) as u64;
    write_varint(out, cursor, body_len) && write_tag(out, cursor, field_number, 2)
}

/// Encode `Data{ portnum, payload }` (field numbers 1/2) backward into `out[..*cursor]`.
///
/// Called in REVERSE wire order — `payload` (field 2) first, `portnum` (field 1) second — so that
/// once the whole message is later flipped right-side-up by [`ToRadioFrame::to_radio_frame`], the
/// final forward byte layout reads `portnum` THEN `payload` (ascending field number, matching the
/// research note's worked byte sequences).
fn encode_data(out: &mut [u8], cursor: &mut usize, portnum: u32, payload: &[u8]) -> bool {
    write_len_delimited_bytes(out, cursor, 2, payload)
        && write_varint_field(out, cursor, 1, u64::from(portnum))
}

/// Encode `MeshPacket{ to, channel, decoded: Data }` (field numbers 2/3/4) backward into
/// `out[..*cursor]`. Reverse wire order: `decoded` first, then `channel`, then `to` last (which
/// therefore lands FIRST in the final forward layout — field 2 before field 3 before field 4).
fn encode_mesh_packet(
    out: &mut [u8],
    cursor: &mut usize,
    channel: u32,
    portnum: u32,
    payload: &[u8],
) -> bool {
    write_len_delimited_nested(out, cursor, 4, |out, cursor| {
        encode_data(out, cursor, portnum, payload)
    }) && write_varint_field(out, cursor, 3, u64::from(channel))
        && write_fixed32_field(out, cursor, 2, NODENUM_BROADCAST)
}

/// Encode `ToRadio{ packet: MeshPacket }` (field number 1, the `payload_variant` oneof's `packet`
/// arm) backward into `out[..*cursor]`.
fn encode_to_radio(
    out: &mut [u8],
    cursor: &mut usize,
    channel: u32,
    portnum: u32,
    payload: &[u8],
) -> bool {
    write_len_delimited_nested(out, cursor, 1, |out, cursor| {
        encode_mesh_packet(out, cursor, channel, portnum, payload)
    })
}

impl ToRadioFrame for MeshFrame<'_> {
    /// Write `0x94 0xC3` + a big-endian `u16` length + the hand-encoded `ToRadio` protobuf message
    /// into `out`. Returns the number of bytes written, or `0` if `out` is too small to hold the
    /// full frame (mirrors [`crate::mesh::condense`]'s "0 = nothing written" convention).
    fn to_radio_frame(&self, out: &mut [u8]) -> usize {
        // Reverse-encode the ToRadio message into the tail of `out` (see module doc comment for
        // why): a declared length > `u16::MAX` or a buffer that runs out of room both fail here,
        // leaving `out`'s eventual contents unspecified but the return value correctly `0`.
        let mut cursor = out.len();
        if !encode_to_radio(out, &mut cursor, self.channel, self.portnum, self.payload) {
            return 0;
        }
        let msg_len = out.len() - cursor;
        if msg_len > usize::from(u16::MAX) {
            return 0;
        }
        let total = FRAME_MAGIC.len() + 2 + msg_len;
        if total > out.len() {
            return 0;
        }

        // `cursor >= 4` is guaranteed here: `total <= out.len()` and `cursor == out.len() -
        // msg_len`, so `cursor == out.len() - (total - 4) >= 4`. A backward `copy_within` (source
        // start >= dest start) is always safe as a memmove regardless of overlap.
        out.copy_within(cursor..cursor + msg_len, 4);
        out[0] = FRAME_MAGIC[0];
        out[1] = FRAME_MAGIC[1];
        let len = msg_len as u16;
        out[2] = (len >> 8) as u8;
        out[3] = (len & 0xFF) as u8;
        total
    }
}

#[cfg(test)]
// Test code may unwrap/expect/panic; the workspace lint gate denies these in production logic
// only.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn varint_single_byte_below_128() {
        let mut out = [0u8; 4];
        let mut cursor = out.len();
        assert!(write_varint(&mut out, &mut cursor, 3));
        assert_eq!(&out[cursor..], &[0x03]);
    }

    #[test]
    fn varint_encodes_multi_byte_values() {
        let mut out = [0u8; 4];
        let mut cursor = out.len();
        // 300 = 0b1_0010_1100 -> low7=0x2C|cont, next=0x02
        assert!(write_varint(&mut out, &mut cursor, 300));
        assert_eq!(&out[cursor..], &[0xAC, 0x02]);
    }

    #[test]
    fn varint_field_omits_zero_value() {
        let mut out = [0u8; 4];
        let mut cursor = out.len();
        assert!(write_varint_field(&mut out, &mut cursor, 3, 0));
        assert_eq!(
            cursor,
            out.len(),
            "a zero-valued field must be omitted, not written as 0x00"
        );
    }

    #[test]
    fn fixed32_field_writes_four_little_endian_bytes_plus_tag() {
        let mut out = [0u8; 8];
        let mut cursor = out.len();
        assert!(write_fixed32_field(&mut out, &mut cursor, 2, 0xFFFF_FFFF));
        assert_eq!(&out[cursor..], &[0x15, 0xFF, 0xFF, 0xFF, 0xFF]);
    }

    /// Golden byte test: `channel = 0` (omitted per proto3 default-value elision), `portnum =
    /// PRIVATE_APP (256)`, `payload = "hi"`. Bytes hand-verified against the worked byte sequences
    /// in the "Meshtastic Serial/UART Protobuf Client API (Heltec V4)" research note.
    #[test]
    fn golden_bytes_channel_zero_omits_the_channel_field() {
        let frame = MeshFrame {
            channel: 0,
            portnum: PORTNUM_PRIVATE_APP,
            payload: b"hi",
        };
        let mut out = [0u8; MAX_RADIO_FRAME];
        let n = frame.to_radio_frame(&mut out);
        let expected: &[u8] = &[
            0x94, 0xC3, 0x00, 0x10, // stream header + 16-byte ToRadio message
            0x0A, 0x0E, // ToRadio.packet (len 14)
            0x15, 0xFF, 0xFF, 0xFF, 0xFF, // MeshPacket.to = 0xFFFFFFFF (fixed32, #386)
            0x22, 0x07, // MeshPacket.decoded (len 7) -- channel field omitted (== 0)
            0x08, 0x80, 0x02, // Data.portnum = 256
            0x12, 0x02, b'h', b'i', // Data.payload = "hi"
        ];
        assert_eq!(&out[..n], expected);
    }

    /// Golden byte test: nonzero `channel = 3` is present as `MeshPacket` field 3, between `to`
    /// (field 2) and `decoded` (field 4).
    #[test]
    fn golden_bytes_nonzero_channel_is_included() {
        let frame = MeshFrame {
            channel: 3,
            portnum: PORTNUM_PRIVATE_APP,
            payload: b"AB",
        };
        let mut out = [0u8; MAX_RADIO_FRAME];
        let n = frame.to_radio_frame(&mut out);
        let expected: &[u8] = &[
            0x94, 0xC3, 0x00, 0x12, // stream header + 18-byte ToRadio message
            0x0A, 0x10, // ToRadio.packet (len 16)
            0x15, 0xFF, 0xFF, 0xFF, 0xFF, // MeshPacket.to = 0xFFFFFFFF (fixed32, #386)
            0x18, 0x03, // MeshPacket.channel = 3
            0x22, 0x07, // MeshPacket.decoded (len 7)
            0x08, 0x80, 0x02, // Data.portnum = 256
            0x12, 0x02, b'A', b'B', // Data.payload = "AB"
        ];
        assert_eq!(&out[..n], expected);
    }

    /// #386: `MeshPacket.to` (field 2, fixed32) must always be the broadcast address, immediately
    /// after the stream header + `ToRadio.packet` tag/length — independent of channel/payload.
    #[test]
    fn golden_bytes_to_field_is_always_broadcast() {
        let frame = MeshFrame {
            channel: 0,
            portnum: PORTNUM_PRIVATE_APP,
            payload: b"hi",
        };
        let mut out = [0u8; MAX_RADIO_FRAME];
        let n = frame.to_radio_frame(&mut out);
        assert!(n > 0);
        // out[0..4] = stream header, out[4..6] = ToRadio.packet tag+len ("0A 0E"), `to` starts
        // right after at index 6.
        assert_eq!(&out[6..11], &[0x15, 0xFF, 0xFF, 0xFF, 0xFF]);
    }

    #[test]
    fn empty_payload_still_encodes_a_well_formed_frame() {
        let frame = MeshFrame {
            channel: 0,
            portnum: PORTNUM_PRIVATE_APP,
            payload: b"",
        };
        let mut out = [0u8; MAX_RADIO_FRAME];
        let n = frame.to_radio_frame(&mut out);
        assert!(n > 0, "an empty payload is still a valid Data message");
        let declared_len = u16::from_be_bytes([out[2], out[3]]) as usize;
        assert_eq!(
            n,
            4 + declared_len,
            "declared length must match bytes actually written"
        );
    }

    #[test]
    fn buffer_too_small_reports_zero_bytes_written() {
        let frame = MeshFrame {
            channel: 0,
            portnum: PORTNUM_PRIVATE_APP,
            payload: b"hi",
        };
        let mut out = [0u8; 4]; // far shorter than the frame this payload now needs
        assert_eq!(frame.to_radio_frame(&mut out), 0);
    }

    #[test]
    fn a_200_byte_condensed_payload_fits_comfortably_under_max_radio_frame() {
        let payload = [b'x'; 200];
        let frame = MeshFrame {
            channel: 7,
            portnum: PORTNUM_PRIVATE_APP,
            payload: &payload,
        };
        let mut out = [0u8; MAX_RADIO_FRAME];
        let n = frame.to_radio_frame(&mut out);
        assert!(n > 0 && n < MAX_RADIO_FRAME);
    }
}
