use embassy_rp::gpio::Output;

/// W5500 reset (RST) pin concrete type.
pub(crate) type Rp2350W5500Reset = Output<'static>;
