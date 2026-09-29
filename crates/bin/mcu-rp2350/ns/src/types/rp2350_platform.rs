use embassy_time::Delay;

use super::embassy_clock::EmbassyClock;
use super::rp2350_de::Rp2350De;
use super::rp2350_publisher::Rp2350Publisher;
use super::rp2350_uart::Rp2350Uart;

/// `pub(crate)` so `core1.rs` (feature `core1-parser`) can name [`Rp2350Platform`] for
/// `run_task`'s signature without relying on the implicit "private-but-descendant-visible"
/// default.
pub(crate) type Rp2350Platform =
    bin_mcu_platform::McuPlatform<Rp2350Uart, Rp2350De, Delay, Rp2350Publisher, EmbassyClock>;
