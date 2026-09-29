/// A byte-level UART seam — the phase-2 forward-compat point. UART1/GP4-GP5 (a second responder
/// node, e.g. an ESP32 leg) or a future RS-232/RS-485 electrical leg (MAX3232/MAX3485) becomes a
/// second `impl UartChannel`, never a rewrite of `pico_seplos_emu_core::seplos_responder` — that
/// crate only ever sees bytes in, bytes out.
///
/// SYNCHRONOUS (blocking), not async — a deliberate divergence from `lib-io`'s
/// `EmbassyUart`/`SerialTransport` (async, DMA+IRQ-driven, built for the master/poller role
/// against a real BMS). This firmware has exactly one job: block on an incoming request, respond,
/// repeat — nothing else needs to run concurrently, so `embassy_rp::uart::Uart<'d, Blocking>` (no
/// DMA channel/IRQ binding required) is the simplest correct choice. `main.rs` still uses
/// `#[embassy_executor::main]` for the entry point/init, matching the repo's "embassy IS the
/// cross-target seam" convention — the single task inside it just never yields, which is fine
/// with nothing else scheduled.
pub trait UartChannel {
    /// Block until exactly one byte is read into `buf[0]`.
    fn read_byte(&mut self, buf: &mut [u8; 1]);
    /// Block until all of `bytes` has been written.
    fn write_all(&mut self, bytes: &[u8]);
}
