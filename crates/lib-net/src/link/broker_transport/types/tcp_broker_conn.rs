//! `std::net::TcpStream`-backed [`super::BrokerConn`] (feature `std`).

/// One accepted client connection over a plain `std::net::TcpStream`. The stream is put into
/// nonblocking mode on construction (see [`super::BrokerListener::accept`]) so [`super::BrokerConn::recv`]
/// (via the [`super::BrokerConn`] impl) never blocks the pump thread; [`super::BrokerConn::send_all`] flips
/// back to blocking for the duration of the call, mirroring the Zephyr shim's accepted-fd
/// contract (`bin/zephyr/src/net_shim.c`'s `sz_sock_accept`: nonblocking accept, but the accepted
/// client fd itself is forced BLOCKING so `sz_sock_send` gets real kernel-managed backpressure).
pub struct TcpBrokerConn {
    pub(in super::super) stream: std::net::TcpStream,
}
