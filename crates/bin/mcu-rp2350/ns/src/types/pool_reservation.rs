//! [`PoolReservation`] — a Drop guard owning one claimed [`SocketBufferPool`] buffer pair.
//! Reserving flips the pair's free-flag `true -> false`; the guard's `Drop` flips it back.
//!
//! **Cancellation safety hinges on this guard.** The pump's `accept_source` future reserves a pair
//! then builds a `TcpSocket` on it before awaiting `accept`. Because the guard is declared BEFORE
//! the socket in that async block, dropping the future (when another branch wins) drops the socket
//! first — releasing its buffer borrow — then the guard second, freeing the pair. No window exists
//! where a freed pair is still borrowed by a live socket. See `main.rs`'s `broker_task`.

use super::socket_buffer_pool::SocketBufferPool;

/// A Drop guard owning one claimed [`SocketBufferPool`] buffer pair (by `index`). Freeing is
/// automatic on drop; on the accept success path the guard is instead moved into a
/// [`crate::types::PooledSocket`], which owns it for the connection's life and preserves the
/// socket-before-slot drop order.
pub struct PoolReservation {
    /// The pool this reservation belongs to (`'static` — the pool lives in a `static`).
    pub(crate) pool: &'static SocketBufferPool,
    /// The claimed pair's index; its free-flag is `false` for this guard's whole life.
    pub(crate) index: usize,
}
