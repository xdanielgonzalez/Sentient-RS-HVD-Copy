//! `impl` blocks for [`super::TcpBrokerListener`] (feature `std`).

use super::{BrokerListener, TcpBrokerConn, TcpBrokerListener};
use std::io::ErrorKind;
use std::net::{TcpListener, ToSocketAddrs};

impl TcpBrokerListener {
    /// Bind + listen on `addr`, and switch to nonblocking mode so [`BrokerListener::accept`]
    /// never blocks — mirrors `sz_sock_listen`'s bind/listen/O_NONBLOCK sequence.
    pub fn bind<A: ToSocketAddrs>(addr: A) -> std::io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        Ok(TcpBrokerListener { listener })
    }
}

impl BrokerListener for TcpBrokerListener {
    type Conn = TcpBrokerConn;

    fn accept(&mut self) -> Option<Self::Conn> {
        match self.listener.accept() {
            Ok((stream, _peer)) => {
                // Force the accepted stream BLOCKING (accept on a nonblocking listener does not
                // guarantee a nonblocking accepted socket on every platform, and we want an
                // explicit, platform-independent guarantee either way) — mirrors
                // `sz_sock_accept` clearing O_NONBLOCK on the Zephyr accepted fd so `send_all`
                // gets real blocking/kernel-managed backpressure. `recv`'s per-call nonblocking
                // behavior is achieved via `MSG_DONTWAIT` in `BrokerConn::recv`, not socket mode
                // — see that impl.
                let _ = stream.set_nonblocking(false);
                Some(TcpBrokerConn { stream })
            }
            // WouldBlock (nothing pending) and any other transient accept error are both "no new
            // connection this tick" to the pump — matches `sz_sock_accept`'s `-1` catch-all.
            Err(e) if e.kind() == ErrorKind::WouldBlock => None,
            Err(_) => None,
        }
    }
}
