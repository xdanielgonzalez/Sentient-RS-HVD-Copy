use super::inverter_command::InverterCommand;
use super::parse_error::ParseError;
use crate::canonical::{
    CanonicalInverterFault, CanonicalInverterReading, InverterMode, InverterRatings,
};

/// An inverter vendor protocol: parse responses into the canonical model and build requests.
///
/// Sibling of [`super::BmsVendorProtocol`] — deliberately NOT a generalization of it. Inverters
/// and BMS devices are different device classes polled over different serial legs (RS-232 vs
/// RS-485) with unrelated wire protocols and unrelated canonical models; the battery trait is
/// also MCU-consumed and must not grow inverter-shaped surface. Implementors are stateless w.r.t.
/// the wire (state — e.g. caching a mode/ratings poll to merge into a reading — lives in the
/// caller's poll driver). All methods are pure — no I/O; the transport adapter supplies `bytes`
/// and transmits `out`.
pub trait InverterVendorProtocol {
    /// Build a request frame for `command` into `out`, returning bytes written.
    fn build_request(
        &self,
        command: InverterCommand,
        out: &mut heapless::Vec<u8, 16>,
    ) -> Result<(), ParseError>;

    /// Validate a raw response frame's integrity (leading/trailing framing bytes + checksum).
    /// Callers MUST call this before any `parse_*` method below; the `parse_*` methods assume an
    /// already-validated frame and do not re-check the checksum.
    fn validate_frame(&self, bytes: &[u8]) -> bool;

    /// Parse a validated status response (Voltronic: `QPIGS`) into a canonical inverter reading.
    /// `mode` and `ratings` are NOT carried by this response; implementors fill them with their
    /// "not yet known" placeholders ([`InverterMode::Unknown`] / `None`) — merging in a
    /// separately polled [`parse_mode`](Self::parse_mode)/[`parse_ratings`](Self::parse_ratings)
    /// result into one composite reading is the caller's concern.
    fn parse_status(&self, bytes: &[u8]) -> Result<CanonicalInverterReading, ParseError>;

    /// Parse a validated mode response (Voltronic: `QMOD`) into the canonical operating mode.
    fn parse_mode(&self, bytes: &[u8]) -> Result<InverterMode, ParseError>;

    /// Parse a validated fault/warning response (Voltronic: `QPIWS`) into canonical fault flags.
    fn parse_fault(&self, bytes: &[u8]) -> Result<CanonicalInverterFault, ParseError>;

    /// Parse a validated ratings response (Voltronic: `QPIRI`) into canonical static ratings.
    fn parse_ratings(&self, bytes: &[u8]) -> Result<InverterRatings, ParseError>;

    /// Parse a validated serial-number response (Voltronic: `QID`) into `out`.
    fn parse_serial(&self, bytes: &[u8], out: &mut heapless::String<32>) -> Result<(), ParseError>;
}
