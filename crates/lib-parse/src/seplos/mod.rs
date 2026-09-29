//! Seplos BMS protocol parser (feature `seplos`).
//!
//! RS-485 ASCII wire protocol: `~`-framed request/response, hex-encoded fields, a 16-bit
//! ASCII-sum two's-complement frame checksum, and a LENGTH field carrying LENID + a checksum
//! nibble. Commands: `0x42` telemetry, `0x44` fault/warning, `0xA4` serial number.
//!
//! The submodules are the wire mechanics; the `Seplos` type wires them into the vendor-neutral
//! [`BmsVendorProtocol`] contract. Correctness bar is the Seplos protocol + real captures.

pub mod checksum;
pub mod command;
pub mod error;
pub mod fault;
pub mod frame;
pub mod serial_number;
pub mod telemetry;

pub use error::SeplosError;

use crate::canonical::{CanonicalFault, CanonicalReading};
use crate::vendor::{BmsCommand, BmsVendorProtocol, ParseError};

/// The Seplos vendor parser. Stateless; implements [`BmsVendorProtocol`].
///
/// # Example: build a byte-exact request frame
///
/// The poll driver hands the built frame to its transport adapter verbatim — the parser owns
/// the framing, checksum, and LENGTH encoding:
///
/// ```rust
/// use lib_parse::seplos::Seplos;
/// use lib_parse::{BmsCommand, BmsVendorProtocol};
///
/// // Telemetry (`0x42`) request for DIP address 0.
/// let mut request = heapless::Vec::<u8, 32>::new();
/// Seplos::new().build_request(0, BmsCommand::Telemetry, &mut request)?;
/// assert_eq!(&request[..], b"~20004642F0010FD67\r");
/// # Ok::<(), lib_parse::ParseError>(())
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct Seplos;

impl Seplos {
    /// Construct a Seplos parser.
    pub const fn new() -> Self {
        Seplos
    }
}

impl From<SeplosError> for ParseError {
    fn from(e: SeplosError) -> Self {
        match e {
            SeplosError::BadSoi | SeplosError::MissingEoi | SeplosError::Truncated => {
                ParseError::BadFraming
            }
            SeplosError::NonHex | SeplosError::NonAscii | SeplosError::InfoUnderrun => {
                ParseError::Malformed
            }
            SeplosError::BadChecksum { .. } | SeplosError::BadLengthChecksum => {
                ParseError::BadChecksum
            }
            SeplosError::LengthMismatch { .. } => ParseError::Malformed,
            SeplosError::RtnError(c) => ParseError::DeviceError(c),
            SeplosError::AddrMismatch { .. } => ParseError::AddressMismatch,
            SeplosError::TooManyElements => ParseError::CapacityExceeded,
            SeplosError::AddrOutOfRange(_) => ParseError::Malformed,
        }
    }
}

fn to_cid2(command: BmsCommand) -> command::Command {
    match command {
        BmsCommand::Telemetry => command::Command::Telemetry,
        BmsCommand::Fault => command::Command::Fault,
        BmsCommand::SerialNumber => command::Command::SerialNumber,
    }
}

impl BmsVendorProtocol for Seplos {
    fn parse_reading(
        &self,
        bytes: &[u8],
        expected_address: u8,
    ) -> Result<CanonicalReading, ParseError> {
        let frame = frame::ResponseFrame::parse(bytes)?;
        if frame.adr != expected_address {
            return Err(ParseError::AddressMismatch);
        }
        Ok(telemetry::decode_reading(&frame)?)
    }

    fn parse_fault(
        &self,
        bytes: &[u8],
        expected_address: u8,
    ) -> Result<CanonicalFault, ParseError> {
        let frame = frame::ResponseFrame::parse(bytes)?;
        if frame.adr != expected_address {
            return Err(ParseError::AddressMismatch);
        }
        Ok(fault::decode_fault(&frame)?)
    }

    fn build_request(
        &self,
        address: u8,
        command: BmsCommand,
        out: &mut heapless::Vec<u8, 32>,
    ) -> Result<(), ParseError> {
        command::build_command(address, to_cid2(command), out)?;
        Ok(())
    }
}
