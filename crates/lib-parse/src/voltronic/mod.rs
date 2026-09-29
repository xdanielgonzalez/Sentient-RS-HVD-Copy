//! Voltronic/MPP-Solar PI30 inverter protocol parser (feature `voltronic`).
//!
//! RS-232 ASCII wire protocol, 2400 baud 8N1 (serial params are the transport's concern, not
//! this parser's): request = command bytes + CRC-16/XMODEM (2 bytes, big-endian) + `\r`; response
//! = `(` + space-separated ASCII payload + CRC-16/XMODEM (2 bytes) + `\r`. Commands: `QPIGS`
//! (status), `QMOD` (mode), `QPIWS` (fault/warning bits), `QPIRI` (ratings), `QID` (serial
//! number). The baseline Python reference DISCARDS the response CRC; this parser VERIFIES it
//! (`validate_frame`) — a real CRC byte can equal `0x0D`, which is the caller's (driver-level)
//! problem to mitigate via continuation reads, not this module's.
//!
//! The submodules are the wire mechanics; the `Voltronic` type wires them into the
//! vendor-neutral [`InverterVendorProtocol`] contract. PROVISIONAL — derived from the legacy
//! Python + protocol docs, no real Voltronic/QPIGS wire captures yet (decision 0001's open risk).

pub mod crc;
pub mod fields;

use crate::canonical::{
    CanonicalInverterFault, CanonicalInverterReading, InverterMode, InverterRatings,
};
use crate::vendor::{InverterCommand, InverterVendorProtocol, ParseError};

/// The Voltronic/MPP-Solar PI30 vendor parser. Stateless; implements [`InverterVendorProtocol`].
///
/// # Example: build a `QPIGS` status request
///
/// The poll driver transmits the built frame verbatim over the RS-232 leg — the parser owns
/// the command bytes, the CRC, and the terminator:
///
/// ```rust
/// use lib_parse::voltronic::Voltronic;
/// use lib_parse::{InverterCommand, InverterVendorProtocol};
///
/// let mut request = heapless::Vec::<u8, 16>::new();
/// Voltronic::new().build_request(InverterCommand::Status, &mut request)?;
/// // "QPIGS" + CRC-16/XMODEM (big-endian) + CR.
/// assert_eq!(&request[..], [b'Q', b'P', b'I', b'G', b'S', 0xB7, 0xA9, b'\r']);
/// # Ok::<(), lib_parse::ParseError>(())
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct Voltronic;

impl Voltronic {
    /// Construct a Voltronic parser.
    pub const fn new() -> Self {
        Voltronic
    }
}

/// The ASCII command bytes Voltronic/PI30 expects for a given logical [`InverterCommand`].
fn command_bytes(command: InverterCommand) -> &'static [u8] {
    match command {
        InverterCommand::Status => b"QPIGS",
        InverterCommand::Mode => b"QMOD",
        InverterCommand::Fault => b"QPIWS",
        InverterCommand::Ratings => b"QPIRI",
        InverterCommand::SerialNumber => b"QID",
    }
}

/// Extract the ASCII payload from a `validate_frame`-verified response: strip the leading `(`
/// and the trailing 2 CRC bytes + `\r` (`bytes[1..n-3]`).
fn payload_str(bytes: &[u8]) -> Result<&str, ParseError> {
    let n = bytes.len();
    if n < 4 || bytes[0] != b'(' {
        return Err(ParseError::BadFraming);
    }
    core::str::from_utf8(&bytes[1..n - 3]).map_err(|_| ParseError::Malformed)
}

impl InverterVendorProtocol for Voltronic {
    fn build_request(
        &self,
        command: InverterCommand,
        out: &mut heapless::Vec<u8, 16>,
    ) -> Result<(), ParseError> {
        out.clear();
        let cmd = command_bytes(command);
        out.extend_from_slice(cmd)
            .map_err(|_| ParseError::CapacityExceeded)?;

        let crc = crc::crc16_xmodem(cmd);
        out.push((crc >> 8) as u8)
            .map_err(|_| ParseError::CapacityExceeded)?;
        out.push((crc & 0xFF) as u8)
            .map_err(|_| ParseError::CapacityExceeded)?;
        out.push(b'\r').map_err(|_| ParseError::CapacityExceeded)?;
        Ok(())
    }

    fn validate_frame(&self, bytes: &[u8]) -> bool {
        let n = bytes.len();
        if n < 4 || bytes[0] != b'(' || bytes[n - 1] != b'\r' {
            return false;
        }
        let expected = crc::crc16_xmodem(&bytes[..n - 3]);
        let actual = u16::from_be_bytes([bytes[n - 3], bytes[n - 2]]);
        expected == actual
    }

    fn parse_status(&self, bytes: &[u8]) -> Result<CanonicalInverterReading, ParseError> {
        fields::parse_status(payload_str(bytes)?)
    }

    fn parse_mode(&self, bytes: &[u8]) -> Result<InverterMode, ParseError> {
        fields::parse_mode(payload_str(bytes)?)
    }

    fn parse_fault(&self, bytes: &[u8]) -> Result<CanonicalInverterFault, ParseError> {
        fields::parse_fault(payload_str(bytes)?)
    }

    fn parse_ratings(&self, bytes: &[u8]) -> Result<InverterRatings, ParseError> {
        fields::parse_ratings(payload_str(bytes)?)
    }

    fn parse_serial(&self, bytes: &[u8], out: &mut heapless::String<32>) -> Result<(), ParseError> {
        fields::parse_serial(payload_str(bytes)?, out)
    }
}
