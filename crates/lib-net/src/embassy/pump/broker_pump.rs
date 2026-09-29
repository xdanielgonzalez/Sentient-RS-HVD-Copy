//! [`broker_slot_cycle`] — one client slot's accept -> recv/feed/drain -> close-and-re-listen
//! cycle, generic over the socket type. Free function — no type/impl split (mirrors
//! `crate::embassy::adapter::pumps`'s bare-async-fn convention; there is no struct to introduce here).
//!
//! `embedded_io_async` covers read/write but not accept/close (those are socket-lifecycle
//! operations, not stream I/O), so this fn takes them as caller-supplied async closures rather than
//! widening the generic bound to a socket-specific trait — this keeps `broker_slot_cycle` reusable
//! against ANY `Read + Write` stream a caller can also open/close (a real `embassy_net::TcpSocket`
//! in deliverable 2/3's call site, or a host mock in principle), matching this deliverable's "must
//! not know anything about embassy-net/TcpSocket specifically" requirement from the drain-and-write
//! core one level up.

use crate::link::BrokerCore;
use embedded_io_async::{Read, Write};

use super::drain_and_write;

/// Run ONE full lifecycle of client slot `slot` against `broker`: accept a connection (via
/// `accept`), then loop recv -> `feed_incoming` -> drain-and-write until the peer closes or a
/// read/write error occurs, then `client_closed(slot)` + `close`. Returns so the caller can loop
/// back around (re-listen) for the slot's next connection — this fn covers exactly one connection,
/// not the accept-forever loop, so a caller wanting "keep serving slot N forever" wraps this in its
/// own `loop { broker_slot_cycle(...).await }` (see `crate::embassy::pump` module docs for why: this keeps
/// the fn itself trivially unit-testable with a single scripted connection, rather than needing a
/// cancellation mechanism to end a test).
///
/// * `accept` — an async closure that suspends until a peer connects on `socket`, mirroring
///   `TcpSocket::accept(port)`. Any `Err` is treated as "this attempt failed, try again" by the
///   caller's own outer loop — `broker_slot_cycle` itself returns without touching `broker` if
///   `accept` fails, since no client ever occupied the slot.
/// * `close` — an async closure that tears the socket back down to a re-listenable state
///   (`TcpSocket::close` + `abort`, or just `close`, depending on what the caller's transport
///   needs) once the connection is done, run AFTER `client_closed(slot)`.
///
/// `rx_buf`/`out` are caller-owned scratch: `rx_buf` is the per-read receive scratch handed to
/// `socket.read()`, `out` is the drain-and-write scratch handed to [`drain_and_write`] (must be at
/// least [`BrokerCore::min_out_len`] for the caller's largest rendered payload — see that fn's doc).
///
/// 8 parameters: `broker`/`slot` (the `BrokerCore` seam), `socket` (the transport),
/// `rx_buf`/`out` (caller-owned scratch — no allocation), `render` (payload rendering), `accept`/
/// `close` (socket lifecycle `embedded_io_async` doesn't cover). Each is independently necessary and
/// not naturally groupable without inventing a struct purely to satisfy this lint (which would then
/// need its own `type_*.rs`/`impl_*.rs` split per the repo's code-organization rule for no real
/// clarity gain) — allowed rather than restructured, matching existing precedent in this workspace
/// (`crates/test-e2e/tests/owned_broker_e2e.rs`, `crates/lib-parse/tests/seplos_telemetry_proptest.rs`).
#[allow(clippy::too_many_arguments)]
pub async fn broker_slot_cycle<S, A, C, R, const MAX_CLIENTS: usize>(
    broker: &mut BrokerCore<MAX_CLIENTS>,
    slot: usize,
    socket: &mut S,
    rx_buf: &mut [u8],
    out: &mut [u8],
    render: &mut R,
    mut accept: A,
    mut close: C,
) where
    S: Read + Write,
    A: AsyncFnMut(&mut S) -> bool,
    C: AsyncFnMut(&mut S),
    R: FnMut(&str, &mut [u8]) -> Option<usize>,
{
    if !accept(socket).await {
        return;
    }
    broker.client_connected(slot);

    loop {
        match socket.read(rx_buf).await {
            Ok(0) => break,  // orderly close (embedded_io_async::Read's EOF contract)
            Err(_) => break, // read error — treat the peer as gone, same as an orderly close
            Ok(n) => {
                // feed_incoming's Err already left the slot doomed/closing internally (see
                // BrokerCore's frozen contract) — draining below still flushes any staged refusal
                // control frame before this cycle ends, so the error is intentionally not
                // propagated further here (there is nothing more for this fn to do with it besides
                // let the drain-then-close path below run its course).
                let _ = broker.feed_incoming(slot, &rx_buf[..n]);
            }
        }

        if drain_and_write(broker, slot, socket, out, render)
            .await
            .is_err()
        {
            break;
        }
    }

    // Final drain: a doomed slot (protocol violation, clean DISCONNECT, or the read/write break
    // above) may still have a staged control frame (e.g. a refusal CONNACK) that must reach the
    // wire before the socket closes — BrokerCore's contract requires drain_outgoing to keep being
    // callable/flushed even after the slot is marked closing (see mqtt_server.rs's `mark_closing`
    // doc). Errors here are ignored: the peer is going away regardless.
    let _ = drain_and_write(broker, slot, socket, out, render).await;

    broker.client_closed(slot);
    close(socket).await;
}
