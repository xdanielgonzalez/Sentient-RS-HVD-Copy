//! `std::net::TcpListener`-backed [`super::BrokerListener`] (feature `std`).

/// A [`super::BrokerListener`] over a plain `std::net::TcpListener`, put into nonblocking mode on
/// construction so [`super::BrokerListener::accept`] never blocks the pump thread — the host
/// analogue of `sz_sock_listen`'s nonblocking-accept setup in `bin/zephyr/src/net_shim.c`. Not
/// `no_std`; only compiled under feature `std`, so it never reaches the MCU no_std build (see
/// `justfile`'s `build-no-std`, which builds this crate with `--no-default-features`).
pub struct TcpBrokerListener {
    pub(in super::super) listener: std::net::TcpListener,
}
