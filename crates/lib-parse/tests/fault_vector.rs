//! T7 — a **synthetic** Seplos `0x44` fault-response vector.
//!
//! No real 0x44 fault capture exists, and the `(byte, bit)` fault map is empirical (see
//! `fault.rs` and the TMS compliance report). This vector is constructed byte-exactly from the
//! Seplos frame spec — VER/ADR/CID1, RTN=00, a valid LENGTH (LCHKSUM + LENID), a fault INFO block
//! with known alarm bits set, and the crate's own frame checksum — so the decoder can be exercised
//! today. If a real capture arrives later it drops into the same assertions.
//!
//! SYNTHETIC: built per spec, not observed on the wire.
#![cfg(feature = "seplos")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_parse::seplos::checksum::{frame_checksum, length_checksum};
use lib_parse::seplos::Seplos;
use lib_parse::vendor::BmsVendorProtocol;

fn push_hex_u8(out: &mut Vec<u8>, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push(HEX[(byte >> 4) as usize]);
    out.push(HEX[(byte & 0xF) as usize]);
}

/// Assemble a byte-exact `0x44` response frame carrying `info` as the fault block.
fn make_fault_frame(adr: u8, info: &[u8]) -> Vec<u8> {
    let lenid = (info.len() * 2) as u16;
    let length: u16 = ((length_checksum(lenid) as u16) << 12) | (lenid & 0x0FFF);

    let mut body = Vec::new(); // VER..INFO
    body.extend_from_slice(b"20");
    push_hex_u8(&mut body, adr);
    body.extend_from_slice(b"46"); // CID1
    push_hex_u8(&mut body, 0x00); // RTN = success
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

/// An 11-byte fault block (indices 0..=10) with a known set of alarm bits:
///  byte 3 bit 0 → high_voltage_protection
///  byte 5 bit 2 → short_circuit_protection
///  byte 7 bit 1 → low_capacity_warning
///  byte 10 bit 1 → charge_current_warning
fn known_fault_info() -> [u8; 11] {
    let mut info = [0u8; 11];
    info[3] = 0b0000_0001;
    info[5] = 0b0000_0100;
    info[7] = 0b0000_0010;
    info[10] = 0b0000_0010;
    info
}

#[test]
fn known_alarm_bits_decode_to_flags() {
    let frame = make_fault_frame(0, &known_fault_info());
    let fault = Seplos::new()
        .parse_fault(&frame, 0)
        .expect("synthetic 0x44 decodes");

    // Set bits.
    assert!(fault.high_voltage_protection);
    assert!(fault.short_circuit_protection);
    assert!(fault.low_capacity_warning);
    assert!(fault.charge_current_warning);
    // Neighbours that must NOT be set (guards against bit/byte off-by-one in the map).
    assert!(!fault.low_voltage_protection);
    assert!(!fault.discharge_overcurrent_protection);
    assert!(!fault.cell_balance_warning);
    assert!(!fault.discharge_current_warning);
    assert_eq!(fault.device_address, 0);
}

#[test]
fn all_clear_frame_has_no_flags() {
    let frame = make_fault_frame(0, &[0u8; 11]);
    let fault = Seplos::new()
        .parse_fault(&frame, 0)
        .expect("all-clear 0x44 decodes");

    assert!(!fault.high_voltage_protection);
    assert!(!fault.low_voltage_protection);
    assert!(!fault.short_circuit_protection);
    assert!(!fault.mosfet_fault);
    assert!(!fault.cell_balance_warning);
    assert!(!fault.high_voltage_warning);
    assert!(!fault.charge_current_warning);
}

#[test]
fn short_info_reads_missing_bytes_as_clear() {
    // A fault block shorter than the mapped indices must not error — missing bytes read as clear.
    let frame = make_fault_frame(0, &[0x00, 0x00, 0x00]); // only bytes 0..=2 present
    let fault = Seplos::new()
        .parse_fault(&frame, 0)
        .expect("short fault block still decodes");
    assert!(!fault.high_voltage_protection); // byte 3 absent → clear
    assert!(!fault.charge_current_warning); // byte 10 absent → clear
}
