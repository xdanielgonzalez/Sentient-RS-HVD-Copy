//! The listening-socket half of the broker-pump transport seam.

use super::BrokerConn;

/// The broker pump's listening socket, as `rust_broker_main` needs it: **nonblocking** accept —
/// at most one new connection is taken per tick, and "nothing pending" must return cheaply rather
/// than block the single pump thread (mirrors `sz_sock_listen`/`sz_sock_accept` in
/// `bin/zephyr/platform/src/lib.rs` + `bin/zephyr/src/net_shim.c`).
///
/// Generic associated connection type (`Conn: BrokerConn`) — implementors are used generically
/// (`<L: BrokerListener>`), never boxed, matching `lib_io::transport::SerialTransport`'s
/// static-dispatch style.
pub trait BrokerListener {
    /// The per-connection type this listener hands out on [`Self::accept`].
    type Conn: BrokerConn;

    /// Nonblocking accept. `Some(conn)` if a new connection was pending this tick, `None`
    /// otherwise — must never block. Note this trait does not attempt to distinguish "nothing
    /// pending" from a transient accept error (mirroring `sz_sock_accept`'s own `-1` catch-all):
    /// both are simply "no new connection this tick" to the pump.
    fn accept(&mut self) -> Option<Self::Conn>;
}
