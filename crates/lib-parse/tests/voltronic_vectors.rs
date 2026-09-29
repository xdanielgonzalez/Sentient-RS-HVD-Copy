//! Voltronic/PI30 full-frame acceptance vectors: byte-exact request frames (CRC ground-truth
//! verified in `voltronic_crc_vectors.rs`), `validate_frame` accept/reject, and an end-to-end
//! response decode via the [`InverterVendorProtocol`] trait.
//!
//! SYNTHETIC: built per the protocol facts, not observed on the wire (decision 0001's open risk).
#![cfg(feature = "voltronic")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_parse::vendor::{InverterCommand, InverterVendorProtocol};
use lib_parse::voltronic::crc::crc16_xmodem;
use lib_parse::voltronic::Voltronic;

// ---- request builder: byte-exact request frames ----

fn build(cmd: InverterCommand) -> heapless::Vec<u8, 16> {
    let mut out = heapless::Vec::new();
    Voltronic::new()
        .build_request(cmd, &mut out)
        .expect("build");
    out
}

#[test]
fn request_frames_are_byte_exact() {
    assert_eq!(
        &build(InverterCommand::Status)[..],
        [b'Q', b'P', b'I', b'G', b'S', 0xB7, 0xA9, b'\r']
    );
    assert_eq!(
        &build(InverterCommand::Mode)[..],
        [b'Q', b'M', b'O', b'D', 0x49, 0xC1, b'\r']
    );
    assert_eq!(
        &build(InverterCommand::Fault)[..],
        [b'Q', b'P', b'I', b'W', b'S', 0xB4, 0xDA, b'\r']
    );
    assert_eq!(
        &build(InverterCommand::Ratings)[..],
        [b'Q', b'P', b'I', b'R', b'I', 0xF8, 0x54, b'\r']
    );
    assert_eq!(
        &build(InverterCommand::SerialNumber)[..],
        [b'Q', b'I', b'D', 0xD6, 0xEA, b'\r']
    );
}

// ---- validate_frame ----

const GOLDEN_QPIGS: &str = "230.0 50.0 230.0 50.0 1000 59 00 380 51.20 010 000 035 005.0 185.0 00000 000 10010110 000 00.0 0500 00000";

fn make_response(payload: &str) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.push(b'(');
    frame.extend_from_slice(payload.as_bytes());
    let crc = crc16_xmodem(&frame);
    frame.push((crc >> 8) as u8);
    frame.push((crc & 0xFF) as u8);
    frame.push(b'\r');
    frame
}

#[test]
fn validate_frame_accepts_well_formed_response() {
    let frame = make_response(GOLDEN_QPIGS);
    assert!(Voltronic::new().validate_frame(&frame));
}

#[test]
fn validate_frame_rejects_missing_leading_paren() {
    let mut frame = make_response(GOLDEN_QPIGS);
    frame[0] = b'X';
    assert!(!Voltronic::new().validate_frame(&frame));
}

#[test]
fn validate_frame_rejects_missing_trailing_cr() {
    let mut frame = make_response(GOLDEN_QPIGS);
    let last = frame.len() - 1;
    frame[last] = b'X';
    assert!(!Voltronic::new().validate_frame(&frame));
}

#[test]
fn validate_frame_rejects_corrupt_crc() {
    let mut frame = make_response(GOLDEN_QPIGS);
    let crc_hi = frame.len() - 3;
    frame[crc_hi] ^= 0x01;
    assert!(!Voltronic::new().validate_frame(&frame));
}

#[test]
fn validate_frame_rejects_too_short() {
    assert!(!Voltronic::new().validate_frame(b"(\r"));
}

/// This payload's response-frame CRC (computed over `(` + payload) happens to be `0x0DE4` — the
/// CRC HIGH byte is literally `0x0D`. On the wire this is exactly the hazard the driver's
/// `read_until` continuation-read logic exists for (`lib-io/src/driver/mod.rs::exchange_inverter`):
/// a naive `read_until(b'\r')` stops at that CRC byte, one byte short of the frame's real `\r`.
/// `validate_frame`/`parse_status` don't care — they only run once the driver has reassembled the
/// WHOLE frame — so this test proves the fully reassembled frame validates and decodes correctly;
/// the reassembly mechanism itself is exercised in lib-io's driver test.
#[test]
fn full_qpigs_frame_whose_crc_byte_equals_0x0d_validates_and_parses() {
    let frame = make_response(GOLDEN_QPIGS);
    let crc_hi = frame[frame.len() - 3];
    assert_eq!(
        crc_hi, 0x0D,
        "sanity: this fixture is only interesting if the CRC high byte hits 0x0D"
    );

    let vendor = Voltronic::new();
    assert!(vendor.validate_frame(&frame));

    let r = vendor.parse_status(&frame).expect("parses");
    assert_eq!(r.ac_output_active_power_w, 59);
    assert_eq!(r.grid_voltage_dv, 2300);
    assert_eq!(r.battery_voltage_cv, 5120);
}

#[test]
fn end_to_end_mode_and_serial_via_trait() {
    let vendor = Voltronic::new();

    let mode_frame = make_response("L");
    assert!(vendor.validate_frame(&mode_frame));
    assert_eq!(
        vendor.parse_mode(&mode_frame).expect("parses"),
        lib_parse::canonical::InverterMode::Line
    );

    let serial_frame = make_response("92932002100294");
    assert!(vendor.validate_frame(&serial_frame));
    let mut serial = heapless::String::<32>::new();
    vendor
        .parse_serial(&serial_frame, &mut serial)
        .expect("parses");
    assert_eq!(serial.as_str(), "92932002100294");
}
