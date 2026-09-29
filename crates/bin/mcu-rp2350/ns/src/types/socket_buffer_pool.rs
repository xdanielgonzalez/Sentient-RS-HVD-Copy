//! [`SocketBufferPool`] — a `'static` pool of TCP rx/tx buffer pairs for the device-hosted broker's
//! per-client [`embassy_net::tcp::TcpSocket`]s (Story S8 #359, EPIC #350). embassy-net borrows a
//! socket's buffers for its whole life and the pump drops sockets on client close without handing
//! them back, so buffers must be `'static` and reclaimed by this pool.
//!
//! **Peak concurrency = [`POOL_N_BUF`] = `MAX_CLIENTS + 1 = 4`** (3 serving + 1 in-flight accept).
//! Deliberately smaller than `StackResources<STACK_SOCKETS>` (`main.rs`), whose count also covers
//! smoltcp's DHCP/DNS sockets, which carry no app buffers — see `main.rs`'s `STACK_SOCKETS` doc.
//!
//! **Reclaim contract.** [`SocketBufferPool::reserve`] returns a [`crate::types::PoolReservation`]
//! guard, freed on drop strictly AFTER the `TcpSocket` built on the pair
//! ([`crate::types::PooledSocket`]'s field order enforces this). This makes cancelling the pump's
//! in-flight accept future safe: the socket releases its buffer borrow before the guard frees the
//! pair. See `PoolReservation`/`PooledSocket` for the full contract.

use core::cell::UnsafeCell;
use core::sync::atomic::AtomicBool;

/// rx buffer length per pair — reused from the previous single-socket path (a rendered telemetry
/// payload is chunked by `write_all` if it exceeds the tx window, never lost).
pub const POOL_RX_LEN: usize = 1024;
/// tx buffer length per pair — larger than rx to hold a rendered telemetry payload.
pub const POOL_TX_LEN: usize = 2048;
/// Number of buffer pairs = peak concurrent `TcpSocket`s = `MAX_CLIENTS` (3) serving + 1 accepting.
/// `4 * (1024 + 2048) = ~12 KiB` of the RP2350's 520 KiB SRAM — comfortable.
pub const POOL_N_BUF: usize = 4;

/// A `'static` pool of [`POOL_N_BUF`] TCP rx/tx buffer pairs, each guarded by an `AtomicBool`
/// free-flag (`true` = free). Placed in a `static` (`SOCKET_POOL` in `main.rs`); its
/// [`SocketBufferPool::reserve`]/`release` hand out and reclaim `&'static mut` views into the
/// buffers, and a per-pair free-flag guarantees exactly one live `&'static mut` view per pair at a
/// time (the exclusivity the `unsafe` in `impl_socket_buffer_pool.rs`/`impl_pool_reservation.rs`
/// relies on).
pub struct SocketBufferPool {
    /// Per-pair free-flag: `true` = available for [`SocketBufferPool::reserve`]. A CAS
    /// `true -> false` claims a pair; `release` stores `true` back.
    pub(crate) free: [AtomicBool; POOL_N_BUF],
    /// Per-pair rx buffers. `UnsafeCell` so a `&'static self` pool can hand out `&'static mut`
    /// views; the free-flag (not the borrow checker) enforces exclusivity.
    pub(crate) rx: [UnsafeCell<[u8; POOL_RX_LEN]>; POOL_N_BUF],
    /// Per-pair tx buffers. Same `UnsafeCell` rationale as `rx`.
    pub(crate) tx: [UnsafeCell<[u8; POOL_TX_LEN]>; POOL_N_BUF],
}

// SAFETY: `SocketBufferPool` lives in a `static` shared across the single embassy executor thread's
// cooperatively-scheduled tasks. The only interior mutability is the per-pair buffers behind
// `UnsafeCell`, and every `&'static mut` view of a pair is gated by that pair's `AtomicBool`
// free-flag (claimed by a `compare_exchange` in `reserve`, returned by `release`), so no two
// reservations ever alias the same pair. The board is single-core and the executor is
// single-threaded, so there is no true parallel access; the atomics make the flag transitions
// well-defined regardless. `UnsafeCell` is the sole reason the auto `Sync` impl is withheld, so we
// assert it manually.
unsafe impl Sync for SocketBufferPool {}
