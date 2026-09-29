use super::impl_seplos_sim_parse::push_hex_byte;
use super::prelude::*;

/// Encode the `0x42` telemetry INFO payload per
/// [`lib_parse::seplos::telemetry::decode_reading`]'s layout: `data_flag`, `command_group`,
/// `num_cells`, N x cell mV, `num_temps`, T x raw temp, then the fixed 19-byte block (current,
/// total_voltage, remaining_capacity, custom_number, typical_capacity, soc, rated_capacity,
/// cycles, soh, port_voltage), then 4 trailing reserved `u16` words a real device also sends
/// after `port_voltage` (verified against the live capture in
/// `crates/lib-parse/tests/seplos_vectors.rs`'s `LIVE_FRAME`: `fff8 0000 0079 004d`).
/// `decode_reading` never reads past `port_voltage`, so these bytes only affect wire-length
/// fidelity, not any decoded field; their real-world meaning isn't documented anywhere reachable
/// and no test-kit consumer compares raw capture bytes, so they're zeroed here rather than
/// fabricated.
///
/// `command_group` echoes the value from the parsed request's INFO field (0 for the repo
/// builder's 1-nibble request shape, which carries no independent group value) — a real device
/// echoes it back in its reply (see syssi's `esphome-seplos-bms` real RX example, which echoes
/// `01`).
pub(super) fn encode_telemetry_info(state: &BatteryState, command_group: u8) -> Vec<u8, 96> {
    let mut info: Vec<u8, 96> = Vec::new();
    let _ = info.push(0x00); // data_flag
    let _ = info.push(command_group);
    let _ = info.push(state.cell_voltages_mv.len() as u8);
    for &mv in state.cell_voltages_mv.iter() {
        let _ = info.extend_from_slice(&mv.to_be_bytes());
    }
    let _ = info.push(state.temperatures_raw.len() as u8);
    for &t in state.temperatures_raw.iter() {
        let _ = info.extend_from_slice(&t.to_be_bytes());
    }
    let _ = info.extend_from_slice(&state.current_ca.to_be_bytes());
    let _ = info.extend_from_slice(&state.total_voltage_cv.to_be_bytes());
    let _ = info.extend_from_slice(&state.remaining_capacity_cah.to_be_bytes());
    let _ = info.push(0x0A); // custom_number (decoder skips this byte; the real capture carries 0x0A)
    let _ = info.extend_from_slice(&state.typical_capacity_cah.to_be_bytes());
    let _ = info.extend_from_slice(&state.soc_dpct.to_be_bytes());
    let _ = info.extend_from_slice(&state.rated_capacity_cah.to_be_bytes());
    let _ = info.extend_from_slice(&state.cycles.to_be_bytes());
    let _ = info.extend_from_slice(&state.soh_dpct.to_be_bytes());
    let _ = info.extend_from_slice(&state.port_voltage_cv.to_be_bytes());
    let _ = info.extend_from_slice(&[0x00u8; 8]); // 4 reserved u16 words, zeroed (see doc comment)
    info
}

/// Encode the `0x44` fault INFO payload per
/// [`lib_parse::seplos::fault::decode_fault`]'s `(byte, bit)` map: a 2-byte header (zeroed),
/// `num_cells` at byte 2, then the fault/warning bits at bytes 3..=10.
pub(super) fn encode_fault_info(faults: &FaultFlags, num_cells: u8) -> Vec<u8, 16> {
    let mut info: Vec<u8, 16> = Vec::new();
    let _ = info.push(0x00); // byte 0: header (unused by the decoder)
    let _ = info.push(0x00); // byte 1: header (unused by the decoder)
    let _ = info.push(num_cells); // byte 2: cell count

    // Bytes 3..=10 (indices 0..=7 of this local array) carry the fault/warning bits.
    let mut bits = [0u8; 8];
    let mut set = |byte: usize, bit: u8, val: bool| {
        if val {
            bits[byte - 3] |= 1 << bit;
        }
    };
    set(3, 0, faults.high_voltage_protection);
    set(3, 1, faults.low_voltage_protection);
    set(4, 0, faults.high_temperature_charge_protection);
    set(4, 1, faults.low_temperature_charge_protection);
    set(5, 0, faults.charge_overcurrent_protection);
    set(5, 1, faults.discharge_overcurrent_protection);
    set(5, 2, faults.short_circuit_protection);
    set(6, 0, faults.mosfet_fault);
    set(7, 0, faults.cell_balance_warning);
    set(7, 1, faults.low_capacity_warning);
    set(8, 0, faults.high_temperature_discharge_warning);
    set(8, 1, faults.low_temperature_discharge_warning);
    set(9, 0, faults.low_voltage_warning);
    set(9, 1, faults.high_voltage_warning);
    set(10, 0, faults.discharge_current_warning);
    set(10, 1, faults.charge_current_warning);

    let _ = info.extend_from_slice(&bits);
    info
}

/// Encode the `0xA4` serial-number INFO payload: a leading `<` (real devices send one; the
/// decoder strips it) followed by the ASCII serial.
pub(super) fn encode_serial_info(serial: &str) -> Vec<u8, 40> {
    let mut info: Vec<u8, 40> = Vec::new();
    let _ = info.push(b'<');
    let _ = info.extend_from_slice(serial.as_bytes());
    info
}

/// Assemble a full `~ VER ADR CID1 RTN LENGTH INFO CHKSUM \r` response frame. Checksum/LENGTH
/// math is [`frame_checksum`]/[`length_field`] — reused from `lib_parse::seplos::checksum`,
/// never re-derived.
pub(super) fn assemble_frame(adr: u8, rtn: u8, info: &[u8]) -> Vec<u8, MAX_FRAME> {
    let mut body: Vec<u8, MAX_FRAME> = Vec::new();
    let _ = body.extend_from_slice(b"20"); // VER
    push_hex_byte(&mut body, adr);
    let _ = body.extend_from_slice(b"46"); // CID1
    push_hex_byte(&mut body, rtn);

    let lenid = (info.len() * 2) as u16; // LENID counts ASCII-hex CHARACTERS, not bytes.
    let length = length_field(lenid);
    push_hex_byte(&mut body, (length >> 8) as u8);
    push_hex_byte(&mut body, (length & 0xFF) as u8);

    for &b in info {
        push_hex_byte(&mut body, b);
    }

    let checksum = frame_checksum(&body);

    let mut frame: Vec<u8, MAX_FRAME> = Vec::new();
    let _ = frame.push(b'~');
    let _ = frame.extend_from_slice(&body);
    push_hex_byte(&mut frame, (checksum >> 8) as u8);
    push_hex_byte(&mut frame, (checksum & 0xFF) as u8);
    let _ = frame.push(b'\r');
    frame
}
