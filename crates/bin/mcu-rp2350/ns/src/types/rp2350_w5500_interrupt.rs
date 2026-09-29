use embassy_rp::gpio::Input;

/// W5500 interrupt (INT) pin concrete type.
pub(crate) type Rp2350W5500Interrupt = Input<'static>;
