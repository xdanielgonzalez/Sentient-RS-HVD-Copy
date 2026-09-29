/// RS-485 half-duplex bus direction (driver-enable). Ignored on full-duplex RS-232.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Transmit,
    Receive,
}
