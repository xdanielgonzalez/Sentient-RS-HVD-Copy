//! CRC-16/XMODEM ground-truth vectors for the Voltronic/PI30 hand-rolled implementation.
//!
//! Derived the bounded-datum way (never guessed): the exact baseline Python `calculate_crc` loop
//! (`example/aws/src/greengrass/telemetry/inverter/inverter_reader.py`) was run via `python3` for
//! each command string below; these are its verbatim outputs.
#![cfg(feature = "voltronic")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_parse::voltronic::crc::crc16_xmodem;

#[test]
fn crc_matches_python_baseline_vectors() {
    assert_eq!(crc16_xmodem(b"QPIGS"), 0xB7A9);
    assert_eq!(crc16_xmodem(b"QMOD"), 0x49C1);
    assert_eq!(crc16_xmodem(b"QPIWS"), 0xB4DA);
    assert_eq!(crc16_xmodem(b"QPIRI"), 0xF854);
    assert_eq!(crc16_xmodem(b"QID"), 0xD6EA);
}

#[test]
fn crc_of_empty_input_is_zero() {
    // init = 0x0000, no bytes consumed -> the initial value is the result.
    assert_eq!(crc16_xmodem(b""), 0x0000);
}

proptest::proptest! {
    /// Systematic-CRC self-check: appending a message's own CRC (big-endian) to itself and
    /// recomputing must always yield a zero remainder — the standard property of this bit-serial
    /// MSB-first construction. Verified separately (in Python) before trusting it as a property;
    /// this exercises the whole byte range/length space the 5 fixed vectors above can't.
    #[test]
    fn appending_own_crc_yields_zero_remainder(
        data in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..64)
    ) {
        let crc = crc16_xmodem(&data);
        let mut appended = data.clone();
        appended.push((crc >> 8) as u8);
        appended.push((crc & 0xFF) as u8);
        proptest::prop_assert_eq!(crc16_xmodem(&appended), 0);
    }
}
