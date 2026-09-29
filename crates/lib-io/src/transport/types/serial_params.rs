use super::parity::Parity;
use super::stop_bits::StopBits;

/// Serial line parameters.
///
/// `SerialParams::default()` is the Seplos BMS line: **19200 8N1**. Other legs override
/// per-field via struct update — e.g. the Voltronic inverter's RS-232 leg runs 2400 8N1
/// (decision 0001):
///
/// ```rust
/// use lib_io::{Parity, SerialParams, StopBits};
///
/// let params = SerialParams::default();
/// assert_eq!(params.baud, 19_200);
/// assert_eq!(params.data_bits, 8);
/// assert_eq!(params.parity, Parity::None);
/// assert_eq!(params.stop_bits, StopBits::One);
///
/// let inverter = SerialParams { baud: 2_400, ..SerialParams::default() };
/// assert_eq!(inverter.baud, 2_400);
/// assert_eq!(inverter.parity, Parity::None); // still 8N1 — only the baud differs
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SerialParams {
    pub baud: u32,
    pub data_bits: u8,
    pub parity: Parity,
    pub stop_bits: StopBits,
}
