use embassy_rp::gpio::Output;

/// RS-485 DE (driver-enable) pin concrete type for this platform's `McuPlatform` instantiation.
pub(crate) type Rp2350De = Output<'static>;
