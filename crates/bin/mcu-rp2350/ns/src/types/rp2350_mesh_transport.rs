use embassy_time::Delay;

use super::rp2350_mesh_de::Rp2350MeshDe;
use super::rp2350_uart::Rp2350Uart;

/// The Heltec mesh-radio UART leg's concrete `EmbassyUart` instantiation (WP3, delight-features).
/// Reuses [`Rp2350Uart`] — the SAME `BufferedUartAdapter` wrapper the Seplos leg uses (a bare
/// newtype over `embassy_rp::uart::BufferedUart`, not tied to any specific UART peripheral
/// instance, so it serves UART1 here exactly as it serves UART0 there) — paired with the no-op
/// [`Rp2350MeshDe`] instead of [`super::Rp2350De`]: this leg has no RS-485 driver-enable pin.
pub(crate) type Rp2350MeshTransport =
    lib_io::embassy_uart::EmbassyUart<Rp2350Uart, Rp2350MeshDe, Delay>;
