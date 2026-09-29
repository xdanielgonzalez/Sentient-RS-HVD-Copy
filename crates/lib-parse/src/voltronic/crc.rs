//! CRC-16/XMODEM — the Voltronic/PI30 frame checksum: polynomial `0x1021`, initial value
//! `0x0000`, no input/output reflection, 2 bytes big-endian on the wire.
//!
//! Hand-rolled (no crc crate dependency) as a direct byte-at-a-time port of the baseline Python
//! `calculate_crc` (`example/aws/src/greengrass/telemetry/inverter/inverter_reader.py`) — the
//! integration tests (`tests/voltronic_crc_vectors.rs`) verify this bit-for-bit against that
//! algorithm's actual `python3` output, the bounded-datum way to prove a CRC (run the real
//! algorithm, assert the exact output), never a guess.

/// Compute CRC-16/XMODEM over `data`.
pub fn crc16_xmodem(data: &[u8]) -> u16 {
    let mut crc: u16 = 0x0000;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}
