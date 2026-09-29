//! T6 — Seplos frame hardening: LENGTH validation, targeted rejections, and property tests.
//!
//! The frame parser now validates the LENGTH field's LCHKSUM nibble and its LENID against the
//! actual INFO length (previously dead code paths), on top of the frame checksum and RTN. These
//! tests build frames with a self-consistent checksum so a single corruption isolates one failure
//! mode, then fuzz the parser to prove it is total (never panics) on arbitrary and mutated input.
#![cfg(feature = "seplos")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_parse::seplos::checksum::frame_checksum;
use lib_parse::seplos::checksum::length_checksum;
use lib_parse::seplos::command::{build_command, Command};
use lib_parse::seplos::error::SeplosError;
use lib_parse::seplos::frame::ResponseFrame;
use proptest::prelude::*;

/// The live 14-cell telemetry response (Seplos 48150-1101-10E, DIP 0).
const LIVE_FRAME: &[u8] = b"~20004600A08E00000E0F360F360F360F360F350F360F360F360F360F370F370F360F360F35060AF10AF90AF90AF30B370B0CFF5C154C36820A465003074650000203E8154CFFF800000079004DDEC8\r";

/// Push `byte` as two uppercase hex chars.
fn push_hex_u8(out: &mut Vec<u8>, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push(HEX[(byte >> 4) as usize]);
    out.push(HEX[(byte & 0xF) as usize]);
}

/// Build a response frame with a valid outer checksum, so a single overridden field isolates one
/// failure mode. `info` is the raw INFO bytes; `lchksum`/`lenid` default to the correct values.
fn make_response(
    adr: u8,
    rtn: u8,
    info: &[u8],
    lchksum: Option<u8>,
    lenid: Option<u16>,
) -> Vec<u8> {
    let info_hex_len = (info.len() * 2) as u16;
    let lenid = lenid.unwrap_or(info_hex_len);
    let lchksum = lchksum.unwrap_or_else(|| length_checksum(lenid));
    let length: u16 = ((lchksum as u16) << 12) | (lenid & 0x0FFF);

    let mut body = Vec::new(); // VER..INFO (the checksum-covered region, ASCII)
    body.extend_from_slice(b"20");
    push_hex_u8(&mut body, adr);
    body.extend_from_slice(b"46");
    push_hex_u8(&mut body, rtn);
    push_hex_u8(&mut body, (length >> 8) as u8);
    push_hex_u8(&mut body, (length & 0xFF) as u8);
    for &b in info {
        push_hex_u8(&mut body, b);
    }

    let cs = frame_checksum(&body);
    let mut frame = Vec::new();
    frame.push(b'~');
    frame.extend_from_slice(&body);
    push_hex_u8(&mut frame, (cs >> 8) as u8);
    push_hex_u8(&mut frame, (cs & 0xFF) as u8);
    frame.push(b'\r');
    frame
}

#[test]
fn valid_synthetic_frame_parses() {
    let f = make_response(0, 0, &[0xAB, 0xCD], None, None);
    let parsed = ResponseFrame::parse(&f).expect("well-formed frame parses");
    assert_eq!(parsed.adr, 0);
    assert_eq!(parsed.rtn, 0);
    assert_eq!(&parsed.info[..], &[0xAB, 0xCD]);
}

#[test]
fn rtn_nonzero_is_device_error() {
    let f = make_response(0, 0x02, &[0xAB, 0xCD], None, None);
    assert_eq!(ResponseFrame::parse(&f), Err(SeplosError::RtnError(0x02)));
}

#[test]
fn bad_length_checksum_rejected() {
    // Correct LENID, wrong LCHKSUM nibble — the outer checksum still validates.
    let good = length_checksum(4);
    let bad = (good ^ 0x1) & 0xF;
    let f = make_response(0, 0, &[0xAB, 0xCD], Some(bad), None);
    assert_eq!(
        ResponseFrame::parse(&f),
        Err(SeplosError::BadLengthChecksum)
    );
}

#[test]
fn lenid_length_mismatch_rejected() {
    // Declare a LENID that doesn't match the 2-byte (4 hex char) INFO, with a matching LCHKSUM so
    // the LENGTH check falls through to the length comparison.
    let f = make_response(0, 0, &[0xAB, 0xCD], None, Some(6));
    assert_eq!(
        ResponseFrame::parse(&f),
        Err(SeplosError::LengthMismatch {
            lenid: 6,
            actual: 4
        }),
    );
}

#[test]
fn non_hex_in_info_rejected() {
    let mut f = make_response(0, 0, &[0xAB, 0xCD], None, None);
    // Corrupt an INFO hex char to a non-hex byte; the frame will now fail (checksum or non-hex),
    // but it must be a typed error, never a panic.
    let idx = 14; // inside the INFO region
    f[idx] = b'G';
    assert!(ResponseFrame::parse(&f).is_err());
}

#[test]
fn oversized_info_is_bounded_error_not_panic() {
    // 300-byte INFO = 600 hex chars, over the 256-byte info Vec cap → TooManyElements, no panic.
    let info = vec![0x5Au8; 300];
    let f = make_response(0, 0, &info, None, None);
    assert_eq!(ResponseFrame::parse(&f), Err(SeplosError::TooManyElements));
}

#[test]
fn missing_eoi_still_parses_leniently() {
    // Timeout-framed reads may drop the trailing CR; the parser accepts an EOI-less frame (a
    // deliberate leniency — see the reserved MissingEoi variant). Strip the '\r' and re-parse.
    let f = make_response(0, 0, &[0xAB, 0xCD], None, None);
    assert_eq!(*f.last().unwrap(), b'\r');
    let no_eoi = &f[..f.len() - 1];
    assert!(
        ResponseFrame::parse(no_eoi).is_ok(),
        "an EOI-less frame parses"
    );
}

#[test]
fn corrupting_the_soi_is_rejected() {
    // Structural corruption IS caught: flipping the frame-start marker (SOI, the first byte) must
    // always be rejected. This is the deterministic corruption-detection guarantee (the proptest
    // above only asserts totality/no-panic, since detection is not total for trailing bytes).
    let mut m = LIVE_FRAME.to_vec();
    m[0] ^= 0xFF;
    assert!(
        ResponseFrame::parse(&m).is_err(),
        "a corrupt SOI (frame-start marker) must be rejected"
    );
}

proptest! {
    /// The parser is total: any input up to 512 bytes returns Ok/Err, never panics.
    #[test]
    fn parse_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = ResponseFrame::parse(&bytes);
    }

    /// The parser is total under corruption: any single-byte mutation of the live frame returns
    /// Ok/Err without panicking, and the pristine frame always decodes. Corruption *detection* is
    /// deliberately NOT total (the parser is lenient about the trailing EOI and can ignore trailing
    /// bytes, so some mutations produce a still-valid frame) — the structural-corruption-is-rejected
    /// property is asserted deterministically in `corrupting_the_soi_is_rejected` below.
    #[test]
    fn single_byte_mutation_never_panics(idx in 0..LIVE_FRAME.len(), xor in 1u8..=255) {
        let mut m = LIVE_FRAME.to_vec();
        m[idx] ^= xor;
        // Never panics regardless of where the corruption lands (proptest fails on any panic).
        let _ = ResponseFrame::parse(&m);
        // The pristine frame always decodes.
        prop_assert!(ResponseFrame::parse(LIVE_FRAME).is_ok());
    }

    /// Every command frame the builder emits is self-consistent: its LENGTH LCHKSUM validates and
    /// its own frame checksum matches, across all DIPs and both poll commands.
    #[test]
    fn command_frames_are_internally_consistent(dip in 0u8..=15) {
        for cmd in [Command::Telemetry, Command::Fault] {
            let mut out = heapless::Vec::<u8, 32>::new();
            build_command(dip, cmd, &mut out).expect("build");
            // Layout: '~' VER ADR CID1 CID2 LENGTH(4) INFO CHKSUM(4) '\r'
            let body = &out[1..out.len() - 5]; // VER..INFO
            let chk = &out[out.len() - 5..out.len() - 1];
            let cs = frame_checksum(body);
            let got = (hexval(chk[0]) << 12) | (hexval(chk[1]) << 8) | (hexval(chk[2]) << 4) | hexval(chk[3]);
            prop_assert_eq!(cs, got as u16, "builder checksum mismatch dip={}", dip);
            // LENGTH is body[8..12]; validate its LCHKSUM against its LENID.
            let length = (hexval(body[8]) << 12) | (hexval(body[9]) << 8) | (hexval(body[10]) << 4) | hexval(body[11]);
            let lenid = (length & 0x0FFF) as u16;
            let lchksum = ((length >> 12) & 0xF) as u8;
            prop_assert_eq!(lchksum, length_checksum(lenid), "builder LCHKSUM mismatch dip={}", dip);
        }
    }
}

fn hexval(b: u8) -> u32 {
    match b {
        b'0'..=b'9' => (b - b'0') as u32,
        b'A'..=b'F' => (b - b'A' + 10) as u32,
        b'a'..=b'f' => (b - b'a' + 10) as u32,
        _ => 0,
    }
}
