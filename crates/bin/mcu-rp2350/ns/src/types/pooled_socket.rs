//! [`PooledSocket`] — an owned `TcpSocket<'static>` paired with the [`PoolReservation`] for the
//! buffers it was built on; the concrete `S` fed to [`lib_net::embassy::async_broker_pump`].
//!
//! **Field order is load-bearing.** Rust drops fields in declaration order, so `sock` (first)
//! drops before `reservation` (second). The socket holds `&'static mut` views into the
//! reservation's buffers; freeing the pair before the socket drops would let a new `reserve()`
//! hand out the same buffers while still borrowed — aliasing `&mut`, UB. **Do not reorder these
//! fields.**

use embassy_net::tcp::TcpSocket;

use super::pool_reservation::PoolReservation;

/// An owned pooled TCP socket: the `TcpSocket` plus the buffer-pair reservation backing it. `Unpin`
/// (both fields are `Unpin`), satisfying the pump's `S: Read + Write + Unpin` bound. The
/// `embedded_io_async` (0.7) `Read`/`Write`/`ErrorType` impls (see `impl_pooled_socket.rs`) delegate
/// straight to the inner `TcpSocket`'s own 0.7 impls.
pub struct PooledSocket {
    /// The live socket. **Declared first so it drops BEFORE `reservation`** — see the module doc.
    pub(crate) sock: TcpSocket<'static>,
    /// The reservation for the pool buffers `sock` borrows. **Declared second so it frees the pool
    /// pair only AFTER `sock` has dropped.** Never read — held purely so its `Drop` (reclaiming the
    /// pool pair) runs when this `PooledSocket` drops; `#[allow(dead_code)]` documents that intent.
    #[allow(dead_code)]
    pub(crate) reservation: PoolReservation,
}
