use lib_net::embassy::W5500Parts;

use super::rp2350_spi_device::Rp2350SpiDevice;
use super::rp2350_w5500_interrupt::Rp2350W5500Interrupt;
use super::rp2350_w5500_reset::Rp2350W5500Reset;

/// `boot_w5500::build_w5500`'s return type. Corrected from an earlier, unused
/// (`#[allow(dead_code)]`) version of this alias that named `Rp2350Spi` (the bare SPI bus) instead
/// of `Rp2350SpiDevice` (the `ExclusiveDevice`-wrapped `SpiDevice` `build_w5500_stack` actually
/// takes and `embassy_net_wiznet::new` monomorphizes its `Runner`/`Device` over) — now genuinely
/// used as `build_w5500`'s `Ok(..)` type.
pub(crate) type Rp2350W5500Parts =
    W5500Parts<'static, Rp2350SpiDevice, Rp2350W5500Interrupt, Rp2350W5500Reset>;
