use super::bms_command::BmsCommand;
use super::parse_error::ParseError;
use crate::canonical::{CanonicalFault, CanonicalReading};

/// A BMS vendor protocol: parse responses into the canonical model and build requests.
///
/// Implementors are stateless w.r.t. the wire (state lives in the caller's poll driver). All
/// methods are pure — no I/O; the transport adapter supplies `bytes` and transmits `out`.
///
/// # Example
///
/// Callers program against the trait (here as a trait object) and never name a concrete vendor
/// outside construction. A response addressed to a different device fails typed, not silently:
///
/// ```rust
/// use lib_parse::seplos::Seplos;
/// use lib_parse::{BmsVendorProtocol, ParseError};
///
/// // A live-captured telemetry response from DIP address 0.
/// let frame: &[u8] = b"~20004600A08E00000E0F360F360F360F360F350F360F360F360F360F370F370F36\
///     0F360F35060AF10AF90AF90AF30B370B0CFF5C154C36820A465003074650000203E8154CFFF8000000\
///     79004DDEC8\r";
/// let vendor: &dyn BmsVendorProtocol = &Seplos::new();
///
/// // The addressed device decodes.
/// let reading = vendor.parse_reading(frame, 0)?;
/// assert_eq!(reading.device_address, 0);
///
/// // The same bytes while polling address 5 → typed mismatch.
/// assert!(matches!(
///     vendor.parse_reading(frame, 5),
///     Err(ParseError::AddressMismatch)
/// ));
/// # Ok::<(), ParseError>(())
/// ```
pub trait BmsVendorProtocol {
    /// Parse a telemetry response for `expected_address` into a canonical reading.
    fn parse_reading(
        &self,
        bytes: &[u8],
        expected_address: u8,
    ) -> Result<CanonicalReading, ParseError>;

    /// Parse a fault response for `expected_address` into canonical fault flags.
    fn parse_fault(&self, bytes: &[u8], expected_address: u8)
        -> Result<CanonicalFault, ParseError>;

    /// Build a request frame for `address` + `command` into `out`, returning bytes written.
    fn build_request(
        &self,
        address: u8,
        command: BmsCommand,
        out: &mut heapless::Vec<u8, 32>,
    ) -> Result<(), ParseError>;
}
