//! Outcome of one [`crate::link::broker_transport::BrokerConn::recv`] call.

/// What a single nonblocking `recv` on a broker-pump client connection produced. Mirrors the
/// three-way outcome the pump's `sz_sock_recv` FFI call already collapses `i32` into (see
/// `bin/zephyr/platform/src/broker_pump.rs`'s `rust_broker_main`: `n == 0` orderly close, `n > 0` bytes,
/// `n < 0` nothing-this-tick) — named explicitly here so the trait's contract doesn't leak a raw
/// signed-length convention into callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecvOutcome {
    /// `n` bytes were read into the caller's buffer (may be a short read — the caller feeds
    /// whatever arrived and waits for the rest, exactly like the Zephyr fd today).
    Bytes(usize),
    /// The peer closed its write side in an orderly fashion (a `0`-length read). The caller
    /// should close this slot.
    Closed,
    /// No data was available this tick (the nonblocking-equivalent of EAGAIN/EWOULDBLOCK), or a
    /// transient error the caller should treat identically: nothing to feed, try again next tick.
    WouldBlock,
}
