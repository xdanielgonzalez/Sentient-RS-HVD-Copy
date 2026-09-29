use embassy_rp::uart::{Blocking, Uart};

/// UART0-backed [`super::UartChannel`] — wraps a blocking `embassy_rp::uart::Uart` bound to
/// `UART0` (GP0=TX, GP1=RX). `Uart<'d, M>` is generic over lifetime + mode only (NOT the
/// peripheral instance — that's type-erased internally by embassy-rp; verified against
/// embassy-rp 0.10.0's own source, `src/uart/mod.rs` line 147: `pub struct Uart<'d, M: Mode>`).
/// Construction lives in `impl_embassy_uart0.rs`.
pub struct EmbassyUart0<'d> {
    pub(crate) uart: Uart<'d, Blocking>,
}
