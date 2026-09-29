use super::impl_parse::push_hex_byte;
use super::prelude::*;

pub(super) fn encode_telemetry_info(state: &BatteryState, command_group: u8) -> Vec<u8, 96> {
    let mut info = Vec::new();

    // byte 0: 0x00 (data_flag)
    let _ = info.push(0x00u8);

    // byte 1: command_group
    let _ = info.push(command_group);

    // byte 2: cell_voltages_mv count
    let _ = info.push(state.cell_voltages_mv.len() as u8);

    // N x u16 BE: cell_voltages_mv
    for &voltage in state.cell_voltages_mv.iter() {
        let _ = info.extend_from_slice(&voltage.to_be_bytes());
    }

    // byte: temperatures_raw count
    let _ = info.push(state.temperatures_raw.len() as u8);

    // T x u16 BE: temperatures_raw
    for &temp in state.temperatures_raw.iter() {
        let _ = info.extend_from_slice(&temp.to_be_bytes());
    }

    // i16 BE: current_ca (signed)
    let _ = info.extend_from_slice(&state.current_ca.to_be_bytes());

    // u16 BE: total_voltage_cv
    let _ = info.extend_from_slice(&state.total_voltage_cv.to_be_bytes());

    // u16 BE: remaining_capacity_cah
    let _ = info.extend_from_slice(&state.remaining_capacity_cah.to_be_bytes());

    // u8: 0x0A (custom_number; decoder skips this byte)
    let _ = info.push(0x0Au8);

    // u16 BE: typical_capacity_cah
    let _ = info.extend_from_slice(&state.typical_capacity_cah.to_be_bytes());

    // u16 BE: soc_dpct
    let _ = info.extend_from_slice(&state.soc_dpct.to_be_bytes());

    // u16 BE: rated_capacity_cah
    let _ = info.extend_from_slice(&state.rated_capacity_cah.to_be_bytes());

    // u16 BE: cycles
    let _ = info.extend_from_slice(&state.cycles.to_be_bytes());

    // u16 BE: soh_dpct
    let _ = info.extend_from_slice(&state.soh_dpct.to_be_bytes());

    // u16 BE: port_voltage_cv
    let _ = info.extend_from_slice(&state.port_voltage_cv.to_be_bytes());

    // 8 bytes: [0x00u8; 8] (4 reserved u16 words, zeroed)
    let _ = info.extend_from_slice(&[0x00u8; 8]);

    info
}

pub(super) fn assemble_frame(adr: u8, rtn: u8, info: &[u8]) -> Vec<u8, MAX_FRAME> {
    // Step 1: build body with VER, ADR, CID1, RTN, LENGTH
    let mut body = Vec::new();

    // Push ASCII "20" (VER)
    let _ = body.extend_from_slice(b"20");

    // Push hex byte for adr
    push_hex_byte(&mut body, adr);

    // Push ASCII "46" (CID1)
    let _ = body.extend_from_slice(b"46");

    // Push hex byte for rtn
    push_hex_byte(&mut body, rtn);

    // Calculate LENGTH field
    let lenid = (info.len() * 2) as u16;
    let length = length_field(lenid);

    // Push hex bytes for length (high byte, then low byte)
    push_hex_byte(&mut body, (length >> 8) as u8);
    push_hex_byte(&mut body, (length & 0xFF) as u8);

    // Step 3: For each byte in info, push hex byte
    for &byte in info.iter() {
        push_hex_byte(&mut body, byte);
    }

    // Step 4: Calculate checksum of body
    let checksum = frame_checksum(&body);

    // Step 5: Assemble final frame
    let mut frame = Vec::new();

    // Push '~'
    let _ = frame.push(b'~');

    // Extend with body
    let _ = frame.extend_from_slice(&body);

    // Push hex bytes for checksum (high byte, then low byte)
    push_hex_byte(&mut frame, (checksum >> 8) as u8);
    push_hex_byte(&mut frame, (checksum & 0xFF) as u8);

    // Push '\r'
    let _ = frame.push(b'\r');

    frame
}
