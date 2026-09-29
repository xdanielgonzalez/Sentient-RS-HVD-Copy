//! Seplos protocol acceptance vectors.
//!
//! Vectors are derived from real device captures + the Seplos protocol (cross-checked against the
//! Python prototype's illustrative frames, but the protocol/captures are the oracle — not Python
//! parity). This is the correctness gate on top of `cargo check`.

#![cfg(feature = "seplos")]
// Test code may unwrap/panic; the workspace denies these in logic only.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_parse::seplos::{command::Command, frame::ResponseFrame, Seplos};
use lib_parse::vendor::BmsVendorProtocol;

/// The canonical live 14-cell telemetry response (Seplos 48150-1101-10E, DIP 0).
const LIVE_FRAME: &[u8] = b"~20004600A08E00000E0F360F360F360F360F350F360F360F360F360F370F370F360F360F35060AF10AF90AF90AF30B370B0CFF5C154C36820A465003074650000203E8154CFFF800000079004DDEC8\r";

// ---- command builder: byte-exact request frames ----

fn build(dip: u8, cmd: Command) -> heapless::Vec<u8, 32> {
    let mut out = heapless::Vec::new();
    lib_parse::seplos::command::build_command(dip, cmd, &mut out).expect("build");
    out
}

#[test]
fn command_frames_are_byte_exact() {
    assert_eq!(&build(0, Command::Telemetry)[..], b"~20004642F0010FD67\r");
    assert_eq!(&build(1, Command::Telemetry)[..], b"~20014642F0011FD65\r");
    assert_eq!(&build(10, Command::Telemetry)[..], b"~200A4642F001AFD45\r");
    assert_eq!(&build(11, Command::Telemetry)[..], b"~200B4642F001BFD43\r");
    assert_eq!(&build(0, Command::Fault)[..], b"~20004644F0010FD65\r");
    assert_eq!(
        &build(0, Command::SerialNumber)[..],
        b"~200046A4F0010FD58\r"
    );
}

#[test]
fn command_rejects_out_of_range_dip() {
    let mut out = heapless::Vec::<u8, 32>::new();
    assert!(lib_parse::seplos::command::build_command(16, Command::Telemetry, &mut out).is_err());
}

// ---- frame parse ----

#[test]
fn parses_live_frame_header() {
    let f = ResponseFrame::parse(LIVE_FRAME).expect("parse live frame");
    assert_eq!(f.adr, 0);
    assert_eq!(f.rtn, 0);
    assert!(!f.info.is_empty());
}

#[test]
fn rejects_missing_soi() {
    assert!(ResponseFrame::parse(b"20004600A08E\r").is_err());
}

#[test]
fn rejects_truncated() {
    assert!(ResponseFrame::parse(b"~200046\r").is_err());
}

#[test]
fn rejects_corrupt_checksum() {
    let mut bad = LIVE_FRAME.to_vec();
    // Flip a byte in the info region; checksum must now fail.
    bad[20] ^= 0x01;
    assert!(ResponseFrame::parse(&bad).is_err());
}

// ---- telemetry decode (via the vendor trait) ----

#[test]
fn decodes_live_telemetry() {
    let r = Seplos::new()
        .parse_reading(LIVE_FRAME, 0)
        .expect("decode telemetry");
    assert_eq!(r.device_address, 0);
    assert_eq!(r.cell_voltages_mv.len(), 14);
    assert_eq!(r.cell_voltages_mv[0], 3894); // 0x0F36
    assert_eq!(*r.cell_voltages_mv.iter().min().unwrap(), 3893);
    assert_eq!(*r.cell_voltages_mv.iter().max().unwrap(), 3895);
    assert_eq!(r.temperatures_raw.len(), 6);
    assert_eq!(r.current_ca, -164); // 0xFF5C signed
    assert_eq!(r.total_voltage_cv, 5452); // 0x154C
    assert_eq!(r.soc_dpct, 775); // 0x0307 -> 77.5%
    assert_eq!(r.soh_dpct, 1000); // 0x03E8 -> 100.0%
    assert_eq!(r.cycles, 2);
}

#[test]
fn parse_reading_rejects_wrong_address() {
    assert!(Seplos::new().parse_reading(LIVE_FRAME, 5).is_err());
}

// ---- checksum unit ----

#[test]
fn length_field_encoding() {
    use lib_parse::seplos::checksum::length_field;
    assert_eq!(length_field(0x001), 0xF001);
    assert_eq!(length_field(0x000), 0x0000);
}
