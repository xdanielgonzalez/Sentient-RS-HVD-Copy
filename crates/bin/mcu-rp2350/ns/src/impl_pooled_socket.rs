//! Impls for [`PooledSocket`]: construction and the `embedded_io_async` **0.7** `Read`/`Write`/
//! `ErrorType` impls the pump's `S: Read + Write` bound requires.
//!
//! The pump ([`lib_net::embassy::async_broker_pump`]) and embassy-net 0.9.1 both speak
//! `embedded-io-async` **0.7** (in this crate that crate is renamed `embedded_io_async_07`; the
//! default `embedded_io_async` here is 0.6, used only by `lib_io`'s UART adapter). embassy-net's
//! `TcpSocket` already impls the 0.7 traits, so these impls just delegate to the inner socket.

use embassy_net::tcp::Error as TcpError;
use embedded_io_async_07::{ErrorType, Read, Write};

use crate::prelude::*;

impl PooledSocket {
    /// Wrap an accepted `TcpSocket` together with the reservation for the pool buffers it was built
    /// on. Ownership of `reservation` transfers here, so the pool pair is freed only when THIS
    /// value drops (client close / pump slot eviction) — and, by field order, strictly after
    /// `sock` drops. See the type's module doc.
    ///
    /// **Unused under `--features tls`, and that is the point.** The only caller is `main.rs`'s
    /// `accept_source` closure, on the branch that hands a *plaintext* socket to the broker pump —
    /// a branch `tls` deliberately compiles out (ADR 0013; see `boot_tls.rs`). The `allow` is
    /// scoped to that feature so genuine dead code is still reported in every other build.
    #[cfg_attr(feature = "tls", allow(dead_code))]
    pub(crate) fn new(
        sock: embassy_net::tcp::TcpSocket<'static>,
        reservation: PoolReservation,
    ) -> Self {
        PooledSocket { sock, reservation }
    }
}

impl ErrorType for PooledSocket {
    type Error = TcpError;
}

impl Read for PooledSocket {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.sock.read(buf).await
    }
}

impl Write for PooledSocket {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.sock.write(buf).await
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.sock.flush().await
    }
}
