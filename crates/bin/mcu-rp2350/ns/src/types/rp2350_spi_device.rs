use embedded_hal_bus::spi::ExclusiveDevice;

use super::rp2350_spi::Rp2350Spi;

/// SPI-device wrapper type alias for the W5500 driver, wrapping [`Rp2350Spi`] with its dedicated
/// chip-select GPIO + delay source.
pub(crate) type Rp2350SpiDevice =
    ExclusiveDevice<Rp2350Spi, embassy_rp::gpio::Output<'static>, embassy_time::Delay>;
