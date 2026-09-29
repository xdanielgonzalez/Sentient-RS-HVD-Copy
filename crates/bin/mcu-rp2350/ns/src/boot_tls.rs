//! TLS interposition point for the device-hosted MQTT broker (feature `tls`, default OFF). This is
//! a fail-closed STUB, not an implementation. See `docs/decisions/0013-transport-security-trust-store.md`
//! (ADR 0013) §1 trait seam, §3 rustls choice, §6 mandatory mTLS, §7 WP-6 staged plan this file is
//! the placeholder for.
//!
//! `lib-tls` is not yet a dependency: its constructors need a `SecureProvider` (ADR 0013 §1), and no
//! type here satisfies that bound until WP-4/WP-5 land the Secure-world backends.
//!
//! [`interpose_tls`] refuses every connection with [`TlsRefusal`]. Its success type is
//! [`core::convert::Infallible`] (uninhabited), so no code path can hand the broker a plaintext
//! socket while `tls` is enabled — a `--features tls` build serves no MQTT clients, by design. WP-6
//! replaces the body with a real `TlsServerStage` handshake and changes `Ok` to a `TlsPooledSocket`;
//! a failed/unpinned-cert handshake keeps returning [`TlsRefusal`] as shipping behavior.

use core::convert::Infallible;

use embassy_net::tcp::TcpSocket;

use crate::prelude::*;

/// Interpose the TLS server stage on a freshly accepted `:1883` connection.
///
/// Returns `Err(`[`TlsRefusal`]`)` on **every** call today — see this module's doc comment. The
/// `Ok` type is [`Infallible`], so the success path is uninhabited and cannot be reached, written,
/// or accidentally introduced while the stage is a stub.
///
/// The socket is aborted (RST) and flushed before returning, so the client sees a refused
/// connection rather than a hung one. Per ADR 0013 §6 the caller's contract is to return `None`
/// from the `accept_source` closure, which the pump's existing `Either3::First(None)` arm already
/// handles — no new error path in `lib-net`.
pub(crate) async fn interpose_tls(sock: &mut TcpSocket<'static>) -> Result<Infallible, TlsRefusal> {
    // Tear the connection down HERE rather than at the call site, so the refusal reason and the
    // teardown that enforces it stay in one place. `abort()` alone only queues the RST —
    // embassy-net's own docs require a completed `flush()` before the socket is dropped or the
    // remote may never learn the connection closed. The flush's own result is discarded: it
    // reports the reset we just caused. (`abort`/`flush` are both INHERENT `TcpSocket` methods —
    // no `embedded_io_async` trait import needed here.)
    sock.abort();
    let _ = sock.flush().await;

    // WP-6 replaces this with the stage construction + handshake drive loop described in the module
    // doc. It cannot be written today: `lib_tls::TlsServerStage::new` takes a `SecureProvider`, and
    // no type in this crate satisfies that bound (see the module doc and `Cargo.toml`'s `tls`
    // comment). Fabricating a `TlsError::Unimplemented` here to look like a real call would be
    // theatre, so the refusal names the actual missing piece instead.
    Err(TlsRefusal::NoSecureProvider)
}
