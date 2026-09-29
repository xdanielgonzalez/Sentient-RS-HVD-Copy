//! Version-bridge impls for [`BufferedUartAdapter`]: re-implements `embedded-io-async` **0.6**
//! `Read`/`Write` (the version `lib_io::EmbassyUart` bounds on) by delegating to embassy-rp 0.10's
//! `BufferedUart`, which implements the **0.7** traits. See `types/buffered_uart_adapter.rs` for why.
//!
//! Imports the io traits directly rather than via `super::prelude::*`: both semver-incompatible
//! `embedded_io_async` versions (0.6 and the renamed 0.7) are in scope here, and keeping both out
//! of the shared prelude avoids an ambiguous glob elsewhere.

use crate::types::BufferedUartAdapter;
use embassy_futures::select::{select, Either};
use embassy_rp::uart::BufferedUart;
// The 0.6 traits — what `EmbassyUart<U, ..>` requires of `U`. `ErrorKind`/`ErrorType` are
// re-exported by embedded-io-async 0.6 from embedded-io 0.6.
use embedded_io_async::{ErrorKind, ErrorType, Read, Write};
// The 0.7 traits `BufferedUart` actually implements — brought into scope (methods only) so the
// delegating calls below resolve to them.
use embedded_io_async_07::{Read as Read07, Write as Write07};

/// Per-`read()` deadline. A healthy Seplos exchange at 19200 8N1 completes in well under 100ms;
/// 500ms per `read()` call is generous headroom while bounding a wedged read (sim silent/incomplete
/// frame) to ~0.5s before `run` errors the cycle and backs off. Because `read_until` (in
/// `lib_io::driver::read_telemetry`) calls `read_exact` — hence this `read()` — one byte at a time,
/// each successful byte resets the deadline; only a genuine stall trips it.
const READ_TIMEOUT_MS: u32 = 500;

impl BufferedUartAdapter {
    /// Wrap a configured `BufferedUart`.
    pub fn new(uart: BufferedUart) -> Self {
        BufferedUartAdapter { uart }
    }
}

impl ErrorType for BufferedUartAdapter {
    /// The concrete underlying `BufferedUart` error carries no information `lib_io::EmbassyUart`
    /// keeps — it maps every transport error to `IoError::Device` regardless — so this bridge
    /// collapses it to `ErrorKind::Other` rather than threading the 0.7 error type across the
    /// version boundary.
    type Error = ErrorKind;
}

impl Read for BufferedUartAdapter {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        // Race the inner read against a deadline: `lib_io::driver::read_telemetry`'s
        // `read_until` loops `read_exact` (hence this `read()`) one byte at a time with NO
        // deadline of its own on the embassy path, so a Seplos sim that sends an incomplete/late
        // frame (or goes silent mid-reply) would otherwise await the next byte forever and wedge
        // `run_task` permanently. Bind the read future first (borrows `self.uart`/`buf`), then
        // select it against the timer so both are consumed by value.
        let read_fut = Read07::read(&mut self.uart, buf);
        let timeout_fut =
            embassy_time::Timer::after(embassy_time::Duration::from_millis(READ_TIMEOUT_MS as u64));
        match select(read_fut, timeout_fut).await {
            Either::First(result) => result.map_err(|e| {
                // Surface the real embassy-rp `uart::Error` (Overrun/Break/Parity/Framing) over
                // defmt/RTT before collapsing it — this is the datum needed to root-cause the
                // intermittent hardware telemetry faults; the collapsed `ErrorKind::Other` return
                // is unchanged (embassy-rp's own `defmt` feature stays OFF here, so
                // `Debug2Format` bridges its plain-`Debug` error across).
                #[cfg(feature = "defmt")]
                defmt::warn!("uart read err: {}", defmt::Debug2Format(&e));
                #[cfg(not(feature = "defmt"))]
                let _ = &e;
                ErrorKind::Other
            }),
            // Timer fired first: the read stalled. Report a timeout (distinct from a hard UART
            // error) so a stall is visible over RTT — `run` treats this like any other read
            // error: dead cycle -> backoff -> retry, letting the device self-heal.
            Either::Second(()) => {
                #[cfg(feature = "defmt")]
                defmt::warn!("uart read timeout ({=u32}ms)", READ_TIMEOUT_MS);
                Err(ErrorKind::TimedOut)
            }
        }
    }
}

impl Write for BufferedUartAdapter {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        Write07::write(&mut self.uart, buf).await.map_err(|e| {
            #[cfg(feature = "defmt")]
            defmt::warn!("uart write err: {}", defmt::Debug2Format(&e));
            #[cfg(not(feature = "defmt"))]
            let _ = &e;
            ErrorKind::Other
        })
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        Write07::flush(&mut self.uart).await.map_err(|e| {
            #[cfg(feature = "defmt")]
            defmt::warn!("uart flush err: {}", defmt::Debug2Format(&e));
            #[cfg(not(feature = "defmt"))]
            let _ = &e;
            ErrorKind::Other
        })
    }
}
