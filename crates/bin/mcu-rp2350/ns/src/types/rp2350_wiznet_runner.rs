use super::rp2350_spi_device::Rp2350SpiDevice;
use super::rp2350_w5500_interrupt::Rp2350W5500Interrupt;
use super::rp2350_w5500_reset::Rp2350W5500Reset;

/// W5500 SPI frame pump runner type alias (for task signatures that need a concrete type).
pub(crate) type Rp2350WiznetRunner = embassy_net_wiznet::Runner<
    'static,
    embassy_net_wiznet::chip::W5500,
    Rp2350SpiDevice,
    Rp2350W5500Interrupt,
    Rp2350W5500Reset,
>;
