use embassy_rp::uart::BufferedUart;

/// Version-bridge newtype over embassy-rp 0.10's [`BufferedUart`].
///
/// **Why this exists — two `embedded-io-async` versions coexist in this graph.** `lib_io::EmbassyUart`
/// bounds its UART generic `U` on `embedded_io_async::Read + Write` resolved through the MAIN
/// sentient-rs workspace's pin (`embedded-io-async = "0.6"`, inherited by `lib-io`/`lib-net`/
/// `bin-mcu-platform` via `workspace = true`). embassy-rp 0.10, however, implements the
/// `embedded-io-async` **0.7** `Read`/`Write` traits on its `BufferedUart` (verified against the
/// registry source: `embassy-rp-0.10.0/src/uart/buffered.rs` has `impl embedded_io_async::Read for
/// BufferedUart` where `embedded-io-async 0.7.0`). 0.6 and 0.7 are semver-incompatible `0.x`
/// crates, so they are two *distinct* traits — `BufferedUart` does not satisfy the 0.6 bound, and
/// neither the app crates nor embassy-rp are ours to move (crates/** APIs are frozen). This newtype
/// is the bridge: it re-implements the **0.6** `Read`/`Write` traits (the ones `EmbassyUart` wants)
/// by delegating to `BufferedUart`'s **0.7** methods (see `impl_buffered_uart_adapter.rs`).
///
/// `BufferedUart` (not the DMA `Uart<Async>`) is used because `Uart<Async>` implements no
/// `embedded_io_async` trait at all in embassy-rp 0.10 — only `BufferedUart` does. Its interrupt
/// handler is `BufferedInterruptHandler<UART0>` and it needs static TX/RX ring buffers, both wired
/// up at the `main` call site.
pub struct BufferedUartAdapter {
    pub uart: BufferedUart,
}
