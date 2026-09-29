//! [`W5500Parts`] — the handles [`super::super::build_w5500_stack`] hands back.

use embassy_net::{Runner as NetRunner, Stack};
use embassy_net_wiznet::chip::W5500;
use embassy_net_wiznet::{Device as WiznetDevice, Runner as WiznetRunner};
use embedded_hal::digital::OutputPin;
use embedded_hal_async::digital::Wait;
use embedded_hal_async::spi::SpiDevice;

/// The three handles a caller needs after [`super::super::build_w5500_stack`]: the `Stack` (for
/// opening sockets / listening the broker on `:1883`), the `embassy_net::Runner` (spawn
/// `net_runner.run().await` — advances IP/DHCP/TCP), and the WIZnet `Runner` (spawn
/// `wiznet_runner.run().await` — the SPI frame pump + link-state poll).
///
/// Generic over the caller's `embedded-hal-async` SPI + `embedded-hal` GPIO — the concrete
/// embassy-rp `Spi`/`Output`/`Input` (or any other chip's HAL) is supplied entirely at the call
/// site, so this leg names no board-specific type (the seam rule: the HAL stays in the bin).
///
/// The two runners must each be driven by their own `#[embassy_executor::task]` — those task fns
/// must be concrete/non-generic (see [`crate::embassy::host`]'s doc), so the concrete task wrappers
/// closing over a specific SPI/pin monomorphization live at the call site, not here.
pub struct W5500Parts<'d, SPI, INT, RST>
where
    SPI: SpiDevice,
    INT: Wait,
    RST: OutputPin,
{
    /// Open TCP sockets against this (e.g. `TcpSocket::accept(1883)` for the device-hosted broker).
    pub stack: Stack<'d>,
    /// Spawn as a task: drives the IP/DHCP/TCP state machine.
    pub net_runner: NetRunner<'d, WiznetDevice<'d>>,
    /// Spawn as a task: the W5500 SPI frame pump + link-state poll.
    pub wiznet_runner: WiznetRunner<'d, W5500, SPI, INT, RST>,
}
