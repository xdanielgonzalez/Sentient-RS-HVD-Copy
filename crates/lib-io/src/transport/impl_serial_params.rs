use super::prelude::*;

impl Default for SerialParams {
    /// Seplos BMS default: 19200 8N1.
    fn default() -> Self {
        SerialParams {
            baud: 19200,
            data_bits: 8,
            parity: Parity::None,
            stop_bits: StopBits::One,
        }
    }
}
