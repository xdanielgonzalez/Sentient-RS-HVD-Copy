use super::prelude::*;

/// Header fields readable straight off a request's fixed byte positions, independent of whether
/// the frame's LENGTH LCHKSUM, frame CHKSUM, or CID2 turn out to be valid. ADR/CID2/LENGTH sit at
/// fixed offsets regardless of those checks, which is what lets [`classify_malformed_request`]
/// name the DIP a NAK should be addressed to even for a request that fails [`parse_request`].
struct RequestHeader<'a> {
    adr: u8,
    cid2: &'a [u8],
    lenid: u16,
    lchksum_ok: bool,
    body: &'a [u8],
}

/// Strip SOI/EOI and read the fixed 12-byte header (`VER ADR CID1 CID2 LENGTH`). Returns `None`
/// only when the frame is too structurally broken to even locate those fields (bad SOI, a
/// truncated header, or non-hex bytes inside it) — never panics.
fn parse_header(bytes: &[u8]) -> Option<RequestHeader<'_>> {
    if bytes.first() != Some(&b'~') {
        return None;
    }
    let rest = &bytes[1..];
    let body = if rest.last() == Some(&b'\r') {
        &rest[..rest.len().saturating_sub(1)]
    } else {
        rest
    };
    // VER(2) ADR(2) CID1(2) CID2(2) LENGTH(4) = 12-byte fixed header, minimum.
    if body.len() < 12 {
        return None;
    }
    let adr = hex_byte(&body[2..4])?;
    let cid2 = &body[6..8];
    let length = hex_u16(&body[8..12])?;
    let lenid = length & 0x0FFF;
    let lchksum = ((length >> 12) & 0xF) as u8;
    let lchksum_ok = lchksum == length_checksum(lenid);
    Some(RequestHeader {
        adr,
        cid2,
        lenid,
        lchksum_ok,
        body,
    })
}

/// Parse a request frame (`~ VER ADR CID1 CID2 LENGTH INFO CHKSUM \r`). Accepts both request
/// shapes seen in practice:
/// * the repo builder's [`lib_parse::seplos::command::build_command`] form — LENID=`001`, a
///   single hex-char INFO holding the DIP nibble (no independent command-group value: extracted
///   as that nibble);
/// * the documented protocol V2.0 / a real device's TX form (e.g. syssi's
///   `esphome-seplos-bms` capture `~20004642E00200FD37`) — LENID=`002`, a 2-hex-char INFO byte
///   holding the command-group/pack selector.
///
/// Verifies SOI, the LENGTH LCHKSUM, and the frame checksum (reusing [`frame_checksum`]); returns
/// `None` on any malformation — never panics.
pub(super) fn parse_request(bytes: &[u8]) -> Option<ParsedRequest> {
    let header = parse_header(bytes)?;
    if !header.lchksum_ok {
        return None;
    }
    let info_hex_len = match header.lenid {
        1 | 2 => header.lenid as usize,
        _ => return None,
    };
    let expected_len = 12 + info_hex_len + 4;
    if header.body.len() != expected_len {
        return None;
    }

    let sum_region = &header.body[..header.body.len() - 4];
    let chk_hex = &header.body[header.body.len() - 4..];
    let expected = frame_checksum(sum_region);
    let got = hex_u16(chk_hex)?;
    if expected != got {
        return None;
    }

    let command = match header.cid2 {
        b"42" => SimCommand::Telemetry,
        b"44" => SimCommand::Fault,
        b"A4" | b"a4" => SimCommand::SerialNumber,
        _ => return None,
    };

    let info_hex = &header.body[12..12 + info_hex_len];
    let command_group = match info_hex_len {
        1 => hex_nibble(info_hex[0])?,
        2 => hex_byte(info_hex)?,
        _ => 0,
    };

    Some(ParsedRequest {
        adr: header.adr,
        command,
        command_group,
    })
}

/// Classify a request that already failed [`parse_request`] into `(adr, rtn_code)` for auto-NAK
/// mode. Only reports the three protocol-defined codes a request-side fault can name here — `02H`
/// frame CHKSUM, `03H` LENGTH LCHKSUM, `04H` invalid CID2 — checked in that precedence order (a
/// request can only fail for one of the first two we detect); anything else (bad SOI, a truncated
/// header, or a LENID shape we don't model) isn't reliably addressable and returns `None` so the
/// caller falls back to silence.
pub(super) fn classify_malformed_request(bytes: &[u8]) -> Option<(u8, u8)> {
    let header = parse_header(bytes)?;
    if !header.lchksum_ok {
        return Some((header.adr, 0x03));
    }
    let info_hex_len = match header.lenid {
        1 | 2 => header.lenid as usize,
        _ => return None,
    };
    let expected_len = 12 + info_hex_len + 4;
    if header.body.len() != expected_len {
        return None;
    }

    let sum_region = &header.body[..header.body.len() - 4];
    let chk_hex = &header.body[header.body.len() - 4..];
    let expected = frame_checksum(sum_region);
    let got = hex_u16(chk_hex)?;
    if expected != got {
        return Some((header.adr, 0x02));
    }

    let known_cid2 = matches!(header.cid2, b"42" | b"44" | b"A4" | b"a4");
    if !known_cid2 {
        return Some((header.adr, 0x04));
    }
    None
}

pub(super) fn nibble_to_hex(n: u8) -> u8 {
    if n < 10 {
        b'0' + n
    } else {
        b'A' + (n - 10)
    }
}

pub(super) fn push_hex_byte(out: &mut Vec<u8, MAX_FRAME>, byte: u8) {
    let _ = out.push(nibble_to_hex((byte >> 4) & 0xF));
    let _ = out.push(nibble_to_hex(byte & 0xF));
}

pub(super) fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

pub(super) fn hex_byte(pair: &[u8]) -> Option<u8> {
    if pair.len() != 2 {
        return None;
    }
    Some((hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?)
}

pub(super) fn hex_u16(hex4: &[u8]) -> Option<u16> {
    if hex4.len() != 4 {
        return None;
    }
    let hi = hex_byte(&hex4[0..2])?;
    let lo = hex_byte(&hex4[2..4])?;
    Some(((hi as u16) << 8) | lo as u16)
}
