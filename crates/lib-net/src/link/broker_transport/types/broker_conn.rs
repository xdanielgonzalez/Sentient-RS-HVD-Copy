//! The per-connection half of the broker-pump transport seam.

use super::{RecvOutcome, SendOutcome};

/// One accepted client connection, as the device-hosted MQTT broker pump needs it. Mirrors the
/// exact per-tick contract `rust_broker_main` already drives against the Zephyr `sz_sock_*` shim
/// (`bin/zephyr/platform/src/lib.rs`): a **nonblocking** `recv` (never stalls the single pump
/// thread waiting for a client that has nothing to say) and a **blocking, full-length** `send_all`
/// (the broker always wants the whole rendered packet on the wire before moving to the next
/// client — a short send means the peer is gone, not a reason to retry piecemeal).
///
/// Implementors are used generically (`<C: BrokerConn>`) — never boxed, matching
/// `lib_io::transport::SerialTransport`'s static-dispatch style.
pub trait BrokerConn {
    /// Nonblocking read into `buf`. Must never block the caller — if no data is available this
    /// tick, return [`RecvOutcome::WouldBlock`] immediately rather than waiting. A `0`-length
    /// underlying read (orderly peer close) is [`RecvOutcome::Closed`]; `n > 0` bytes read is
    /// [`RecvOutcome::Bytes`] (may be short — the caller feeds what arrived and calls again
    /// next tick for the rest, exactly like today's Zephyr fd).
    fn recv(&mut self, buf: &mut [u8]) -> RecvOutcome;

    /// Blocking write of the *entire* `buf`. The impl is responsible for internally resuming
    /// across any legal short writes (a correctly-configured blocking stream socket may still
    /// legally return a short count — see `net_shim.c`'s `sz_sock_send` doc comment for why) —
    /// callers never see a partial-send outcome, only [`SendOutcome::Sent`] (all of `buf` is
    /// confirmed on the wire) or [`SendOutcome::PeerGone`] (the peer died partway through; the
    /// caller must close this slot, since the MQTT byte stream is now desynced regardless of how
    /// many bytes actually made it out).
    fn send_all(&mut self, buf: &[u8]) -> SendOutcome;

    /// Tear down this connection. Idempotent from the caller's perspective — called at most once
    /// per connection by the pump, but implementors should not assume more than that.
    fn close(&mut self);
}
