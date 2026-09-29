use super::prelude::*;

struct RequestHeader<'a> {
    adr: u8,
    cid2: &'a [u8],
    lenid: u16,
    lchksum_ok: bool,
    body: &'a [u8],
}

fn parse_header(bytes: &[u8]) -> Option<RequestHeader<'_>> {
    // SOI check: first byte must be '~'
    if bytes.first() != Some(&b'~') {
        return None;
    }

    // Strip optional trailing CR
    let mut body = &bytes[1..];
    if body.last() == Some(&b'\r') {
        body = &body[..body.len() - 1];
    }

    // Minimum fixed header size
    if body.len() < 12 {
        return None;
    }

    // Parse fixed header fields
    let adr = hex_byte(&body[2..4])?;
    let cid2 = &body[6..8];
    let length = hex_u16(&body[8..12])?;

    // Extract length ID and checksum
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

pub(super) fn parse_request(bytes: &[u8]) -> Option<u8> {
    let header = parse_header(bytes)?;

    // Length checksum must be valid
    if !header.lchksum_ok {
        return None;
    }

    // Determine info hex length: 1 or 2 only
    let info_hex_len = match header.lenid {
        1 | 2 => header.lenid as usize,
        _ => return None,
    };

    // Verify frame length: 12-byte header + info + 4-byte checksum
    let expected_len = 12 + info_hex_len + 4;
    if header.body.len() != expected_len {
        return None;
    }

    // Verify frame checksum
    let sum_region = &header.body[..header.body.len() - 4];
    let chk_hex = &header.body[header.body.len() - 4..];
    let expected = frame_checksum(sum_region);
    let got = hex_u16(chk_hex)?;
    if expected != got {
        return None;
    }

    // Telemetry-only: only CID2 "42" is recognized
    if header.cid2 != b"42" {
        return None;
    }

    Some(header.adr)
}

pub(super) fn nibble_to_hex(n: u8) -> u8 {
    match n & 0x0F {
        0..=9 => b'0' + n,
        10..=15 => b'A' + (n - 10),
        _ => unreachable!(),
    }
}

pub(super) fn push_hex_byte(out: &mut Vec<u8, MAX_FRAME>, byte: u8) {
    let _ = out.push(nibble_to_hex(byte >> 4));
    let _ = out.push(nibble_to_hex(byte & 0x0F));
}

pub(super) fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(10 + b - b'A'),
        b'a'..=b'f' => Some(10 + b - b'a'),
        _ => None,
    }
}

pub(super) fn hex_byte(pair: &[u8]) -> Option<u8> {
    if pair.len() != 2 {
        return None;
    }
    let hi = hex_nibble(pair[0])?;
    let lo = hex_nibble(pair[1])?;
    Some((hi << 4) | lo)
}

pub(super) fn hex_u16(hex4: &[u8]) -> Option<u16> {
    if hex4.len() != 4 {
        return None;
    }
    let hi = hex_byte(&hex4[0..2])?;
    let lo = hex_byte(&hex4[2..4])?;
    Some(((hi as u16) << 8) | (lo as u16))
}
