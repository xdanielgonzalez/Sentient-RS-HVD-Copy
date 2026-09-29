//! Impls for [`PoolReservation`]: the `unsafe` buffer accessor and the `Drop` that reclaims the
//! pool pair.

use crate::prelude::*;

impl PoolReservation {
    /// Borrow this reservation's claimed rx/tx buffer views to build a `TcpSocket` on.
    ///
    /// # Safety
    /// Must be called at most once on a given reservation (the returned `&'static mut` alias the
    /// pool storage; a second call would produce a second `&mut` to the same buffers). Holding this
    /// reservation guarantees no OTHER reservation can hand out the same pair — see
    /// [`crate::types::SocketBufferPool::buffers`]'s own safety contract, which this forwards to.
    ///
    /// `clippy::mut_from_ref` is expected: the `&mut` views come from the pool's interior mutability,
    /// gated by this reservation's held pair (see the forwarded contract), not from `&self` itself.
    #[allow(clippy::mut_from_ref)]
    pub(crate) unsafe fn buffers(&self) -> (&'static mut [u8], &'static mut [u8]) {
        // SAFETY: this reservation holds `self.index` (its free-flag is false), satisfying
        // `SocketBufferPool::buffers`'s precondition; the caller upholds the call-once requirement.
        self.pool.buffers(self.index)
    }
}

impl Drop for PoolReservation {
    /// Reclaim the buffer pair. By the time this runs, the `TcpSocket` built on the pair has
    /// already dropped — guaranteed by [`crate::types::PooledSocket`]'s field order (socket field
    /// declared before this guard) on the success path, and by the accept async block's `let`
    /// declaration order (guard bound before socket) on the cancellation/error path. So no live
    /// socket still borrows the buffers when they are marked free for the next `reserve`.
    fn drop(&mut self) {
        self.pool.release(self.index);
    }
}
