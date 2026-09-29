//! Impls for [`SocketBufferPool`]: `const` construction, reserve/release of buffer pairs, and the
//! `unsafe` accessor that hands a claimed pair's `&'static mut` rx/tx views to the socket builder.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::prelude::*;

impl SocketBufferPool {
    /// An all-free pool, ready for `static` placement (never a per-call stack local — the repo's
    /// zero-copy/stack-discipline rule; the buffers must be `'static` anyway, see the type doc).
    pub const fn new() -> Self {
        SocketBufferPool {
            free: [const { AtomicBool::new(true) }; POOL_N_BUF],
            rx: [const { UnsafeCell::new([0u8; POOL_RX_LEN]) }; POOL_N_BUF],
            tx: [const { UnsafeCell::new([0u8; POOL_TX_LEN]) }; POOL_N_BUF],
        }
    }

    /// Claim the first free buffer pair, returning a [`PoolReservation`] guard that owns it until
    /// dropped. Returns `None` if every pair is in use — the caller (the pump's `accept_source`)
    /// then backs off briefly rather than busy-spinning. The `compare_exchange` (`AcqRel` on
    /// success) makes the `true -> false` claim atomic against a concurrent `release`, so a pair is
    /// never handed to two reservations.
    pub fn reserve(&'static self) -> Option<PoolReservation> {
        for index in 0..POOL_N_BUF {
            if self.free[index]
                .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Some(PoolReservation { pool: self, index });
            }
        }
        None
    }

    /// Mark a pair free again. Called only from [`PoolReservation`]'s `Drop`, after the socket that
    /// borrowed the pair's buffers has already dropped (see `PooledSocket`'s field-order contract).
    /// `Release` publishes the buffers-are-idle transition to a subsequent `reserve`'s `Acquire`.
    pub(crate) fn release(&self, index: usize) {
        self.free[index].store(true, Ordering::Release);
    }

    /// Hand out the `&'static mut` rx/tx views of a claimed pair.
    ///
    /// # Safety
    /// The caller must hold the [`PoolReservation`] for `index` (i.e. its free-flag is `false`),
    /// and must call this at most once per reservation. That flag guarantees no other reservation
    /// can be handing out the same pair, so the returned `&'static mut` are non-aliasing for as
    /// long as the reservation (and any `TcpSocket` built on them) live. The pool storage is
    /// `'static` (a `static`), so the `'static` lifetime on the returned slices is sound.
    ///
    /// `clippy::mut_from_ref` is expected and intended here: this is precisely a pool handing out an
    /// interior-mutable view from a shared `&self`. The `unsafe` contract above (caller holds the
    /// pair's reservation ⇒ exclusive) is what makes the `&mut` sound; the lint can't see that.
    #[allow(clippy::mut_from_ref)]
    pub(crate) unsafe fn buffers(
        &'static self,
        index: usize,
    ) -> (&'static mut [u8], &'static mut [u8]) {
        // SAFETY (per the fn contract above): exclusive access to pair `index` is guaranteed by the
        // caller's held reservation, so producing `&mut` from these `UnsafeCell`s does not alias.
        let rx: &'static mut [u8; POOL_RX_LEN] = &mut *self.rx[index].get();
        let tx: &'static mut [u8; POOL_TX_LEN] = &mut *self.tx[index].get();
        (&mut rx[..], &mut tx[..])
    }
}

impl Default for SocketBufferPool {
    fn default() -> Self {
        Self::new()
    }
}
