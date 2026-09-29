use embassy_rp::spi::{self, Spi};

/// W5500 concrete type alias: SPI over embassy-rp peripherals (RP2350 Cortex-M33, SPI0). The mode
/// parameter is `Async` for DMA-based async operation.
pub(crate) type Rp2350Spi = Spi<'static, embassy_rp::peripherals::SPI0, spi::Async>;
