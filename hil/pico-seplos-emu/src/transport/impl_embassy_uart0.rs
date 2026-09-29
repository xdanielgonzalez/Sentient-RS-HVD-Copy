use embassy_rp::uart::{Config, DataBits, Parity, StopBits, Uart};
use embassy_rp::Peri;

use super::types::EmbassyUart0;
use super::UartChannel;

impl<'d> EmbassyUart0<'d> {
    /// Construct the UART0 responder channel: GP0=TX, GP1=RX, 19200 8N1 — the Seplos wire
    /// config, verified against `crates/lib-io/src/transport/impl_serial_params.rs`'s
    /// `SerialParams::default()` doc ("the Seplos BMS line: 19200 8N1") and
    /// `crates/bin/rpi/src/config/types/serial_config.rs`'s defaults (`baud: 19200,
    /// data_bits: 8` hardcoded, `parity: None`, `stop_bits: 1`).
    ///
    /// `Peri<'d, T>` types (verified against embassy-rp 0.10.0's `embassy_rp::init()` return
    /// type, `Peripherals`): `uart0: Peri<'d, embassy_rp::peripherals::UART0>`,
    /// `tx: Peri<'d, embassy_rp::peripherals::PIN_0>`, `rx: Peri<'d, embassy_rp::peripherals::PIN_1>`
    /// (PIN_0/PIN_1 are UART0's TX/RX pins — verified: embassy-rp 0.10.0
    /// `src/uart/mod.rs` `impl_pin!(PIN_0, UART0, TxPin); impl_pin!(PIN_1, UART0, RxPin);`).
    ///
    /// `Uart::new_blocking` — verified signature, embassy-rp 0.10.0 `src/uart/mod.rs`:
    /// `impl<'d> Uart<'d, Blocking> { pub fn new_blocking<T: Instance>(uart: Peri<'d, T>, tx:
    /// Peri<'d, impl TxPin<T>>, rx: Peri<'d, impl RxPin<T>>, config: Config) -> Self }` — no DMA
    /// channel, no interrupt binding needed (that's only required for `Uart<'d, Async>`).
    pub fn new(
        uart0: Peri<'d, embassy_rp::peripherals::UART0>,
        tx: Peri<'d, embassy_rp::peripherals::PIN_0>,
        rx: Peri<'d, embassy_rp::peripherals::PIN_1>,
    ) -> Self {
        // `Config` is `#[non_exhaustive]` — external crates can't use struct-literal syntax
        // (even with `..Default::default()`) to construct it, only `Default::default()` +
        // field assignment (verified: E0639 on the struct-literal form).
        let mut config = Config::default();
        config.baudrate = 19200;
        config.data_bits = DataBits::DataBits8;
        config.stop_bits = StopBits::STOP1;
        config.parity = Parity::ParityNone;
        EmbassyUart0 {
            uart: Uart::new_blocking(uart0, tx, rx, config),
        }
    }
}

impl<'d> UartChannel for EmbassyUart0<'d> {
    /// `Uart::blocking_read(&mut self, buffer: &mut [u8]) -> Result<(), Error>` — verified,
    /// embassy-rp 0.10.0 `src/uart/mod.rs` line 1090 (the bidirectional `Uart<'d, M>`'s own
    /// method, works for `M = Blocking`). A framing/parity/overrun `Error` on the wire is not
    /// independently actionable here (the responder's own frame-checksum validation is the real
    /// correctness gate) — loop and retry rather than propagate, mirroring how a real bus just
    /// waits for the next clean byte.
    fn read_byte(&mut self, buf: &mut [u8; 1]) {
        loop {
            if self.uart.blocking_read(buf).is_ok() {
                return;
            }
        }
    }

    /// `Uart::blocking_write(&mut self, buffer: &[u8]) -> Result<(), Error>` — verified,
    /// embassy-rp 0.10.0 `src/uart/mod.rs` line 1080. TX errors on a point-to-point bench wire
    /// with no other bus master are not expected in normal operation; `unwrap` here would violate
    /// the repo's no-panic posture, so retry-until-success mirrors `read_byte` above rather than
    /// silently dropping the reply.
    fn write_all(&mut self, bytes: &[u8]) {
        while self.uart.blocking_write(bytes).is_err() {}
    }
}
