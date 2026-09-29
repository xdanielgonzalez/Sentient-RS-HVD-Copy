//! Property tests for the Seplos `0x42` telemetry decode: valid cell counts (1..=16) across the
//! full `u16` voltage range, and frame-checksum boundary conditions (all-zero / max-value data).
//! Complements `seplos_hardening.rs` (which fuzzes arbitrary/mutated bytes for panic-freedom) by
//! asserting *correctness* on well-formed frames at the shape/value extremes.
#![cfg(feature = "seplos")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_parse::canonical::MAX_CELLS;
use lib_parse::seplos::checksum::{frame_checksum, length_checksum};
use lib_parse::seplos::error::SeplosError;
use lib_parse::seplos::frame::ResponseFrame;
use lib_parse::seplos::telemetry;
use proptest::prelude::*;

fn push_hex_u8(out: &mut Vec<u8>, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push(HEX[(byte >> 4) as usize]);
    out.push(HEX[(byte & 0xF) as usize]);
}

fn push_hex_u16(out: &mut Vec<u8>, v: u16) {
    push_hex_u8(out, (v >> 8) as u8);
    push_hex_u8(out, (v & 0xFF) as u8);
}

/// Assemble a byte-exact, checksum-valid Seplos `0x42` telemetry response for `cells` (voltages,
/// mV) and the fixed 19-byte tail block (`current_ca` is the only signed field). No temperature
/// sensors are populated (`num_temps = 0`) — telemetry cell-count/voltage coverage is the target
/// here; `fault_vector.rs`/`seplos_vectors.rs` already cover the temperature block.
#[allow(clippy::too_many_arguments)]
fn make_telemetry_frame(
    adr: u8,
    cells: &[u16],
    current_ca: i16,
    total_voltage_cv: u16,
    remaining_capacity_cah: u16,
    typical_capacity_cah: u16,
    soc_dpct: u16,
    rated_capacity_cah: u16,
    cycles: u16,
    soh_dpct: u16,
    port_voltage_cv: u16,
) -> Vec<u8> {
    let mut info = Vec::new();
    info.push(0x00); // data_flag (skip)
    info.push(0x00); // command_group (skip)
    info.push(cells.len() as u8); // num_cells
    for &v in cells {
        info.extend_from_slice(&v.to_be_bytes());
    }
    info.push(0x00); // num_temps = 0
    info.extend_from_slice(&current_ca.to_be_bytes());
    info.extend_from_slice(&total_voltage_cv.to_be_bytes());
    info.extend_from_slice(&remaining_capacity_cah.to_be_bytes());
    info.push(0x00); // custom_number (skip)
    info.extend_from_slice(&typical_capacity_cah.to_be_bytes());
    info.extend_from_slice(&soc_dpct.to_be_bytes());
    info.extend_from_slice(&rated_capacity_cah.to_be_bytes());
    info.extend_from_slice(&cycles.to_be_bytes());
    info.extend_from_slice(&soh_dpct.to_be_bytes());
    info.extend_from_slice(&port_voltage_cv.to_be_bytes());

    let lenid = (info.len() * 2) as u16;
    let length: u16 = ((length_checksum(lenid) as u16) << 12) | (lenid & 0x0FFF);

    let mut body = Vec::new(); // VER..INFO (checksum-covered region)
    body.extend_from_slice(b"20"); // VER
    push_hex_u8(&mut body, adr);
    body.extend_from_slice(b"46"); // CID1
    push_hex_u8(&mut body, 0x00); // RTN = success
    push_hex_u16(&mut body, length);
    for &b in &info {
        push_hex_u8(&mut body, b);
    }

    let cs = frame_checksum(&body);
    let mut frame = Vec::new();
    frame.push(b'~');
    frame.extend_from_slice(&body);
    push_hex_u16(&mut frame, cs);
    frame.push(b'\r');
    frame
}

proptest! {
    /// Any cell count in the valid 1..=16 range, with voltages spanning the full `u16` range
    /// (including 0 and `u16::MAX`), decodes to exactly the encoded values — length and content
    /// both round-trip.
    #[test]
    fn valid_cell_count_and_voltage_round_trip(
        cells in proptest::collection::vec(any::<u16>(), 1..=MAX_CELLS)
    ) {
        let frame_bytes = make_telemetry_frame(0, &cells, 0, 0, 0, 0, 0, 0, 0, 0, 0);
        let frame = ResponseFrame::parse(&frame_bytes).expect("well-formed frame parses");
        let reading = telemetry::decode_reading(&frame).expect("well-formed telemetry decodes");

        prop_assert_eq!(reading.cell_voltages_mv.len(), cells.len());
        for (i, &v) in cells.iter().enumerate() {
            prop_assert_eq!(reading.cell_voltages_mv[i], v, "cell {} mismatch", i);
        }
        prop_assert_eq!(reading.device_address, 0);
    }

    /// A cell count over `MAX_CELLS` is always rejected with `TooManyElements`, never a panic,
    /// regardless of the voltage values carried.
    #[test]
    fn over_capacity_cell_count_is_rejected(
        extra in 1usize..=8,
        fill in any::<u16>(),
    ) {
        let cells = vec![fill; MAX_CELLS + extra];
        let frame_bytes = make_telemetry_frame(0, &cells, 0, 0, 0, 0, 0, 0, 0, 0, 0);
        let result = telemetry::decode_reading(
            &ResponseFrame::parse(&frame_bytes).expect("frame header/checksum is self-consistent"),
        );
        prop_assert_eq!(result, Err(SeplosError::TooManyElements));
    }

    /// Checksum boundary: an all-zero body (any length up to a generous bound) always checksums
    /// to zero — `!0 + 1` wraps back to `0x0000` — and that invariant is what a corrupt-to-zero
    /// frame region would (harmlessly) satisfy.
    #[test]
    fn checksum_all_zero_body_is_zero(len in 0usize..=512) {
        let body = vec![0u8; len];
        prop_assert_eq!(frame_checksum(&body), 0);
    }

    /// Checksum boundary: an all-max-value (`0xFF`) INFO payload, at every valid cell count,
    /// builds a self-consistent frame (checksum computed by the same function the parser
    /// verifies with) that parses and decodes to the expected max-value voltages, and never
    /// panics at the numeric extremes of every field (including the signed `current_ca` field
    /// at its `i16::MIN` bit pattern).
    #[test]
    fn checksum_max_value_body_round_trips(num_cells in 1usize..=MAX_CELLS) {
        let cells = vec![u16::MAX; num_cells];
        let frame_bytes = make_telemetry_frame(
            0,
            &cells,
            i16::MIN,
            u16::MAX,
            u16::MAX,
            u16::MAX,
            u16::MAX,
            u16::MAX,
            u16::MAX,
            u16::MAX,
            u16::MAX,
        );
        let frame = ResponseFrame::parse(&frame_bytes).expect("max-value frame parses");
        let reading = telemetry::decode_reading(&frame).expect("max-value telemetry decodes");

        prop_assert_eq!(reading.cell_voltages_mv.len(), num_cells);
        prop_assert!(reading.cell_voltages_mv.iter().all(|&v| v == u16::MAX));
        prop_assert_eq!(reading.current_ca, i16::MIN);
        prop_assert_eq!(reading.total_voltage_cv, u16::MAX);
        prop_assert_eq!(reading.port_voltage_cv, u16::MAX);
    }
}
