//! The device-hosted MQTT broker pump's **transport seam** — [`BrokerListener`]/[`BrokerConn`],
//! programmed against generically (`<L: BrokerListener>`, no `Box<dyn>`) by the pump loop
//! (`bin/zephyr/platform/src/broker_pump.rs`'s `rust_broker_main`) instead of calling the Zephyr
//! `sz_sock_*` FFI shim directly. Mirrors `lib_io::transport::SerialTransport`'s
//! host-impl-plus-target-impl-behind-one-trait shape (see that crate's docs) — see the
//! Abstraction & Platform Seams section of the repo's `CLAUDE.md` for why this seam exists (a
//! prior broker revision drove `sz_sock_*` straight from the pump, un-host-testable).
//!
//! `no_std` by default (this module's core — [`BrokerListener`]/[`BrokerConn`]/[`RecvOutcome`]/
//! [`SendOutcome`] — pulls in nothing but `core`, so the Zephyr impl in `impl-app` can implement
//! it without pulling `std` into the MCU build). The `std::net::TcpListener`/`TcpStream` host
//! impl (`TcpBrokerListener`/`TcpBrokerConn`) and the scripted constrained-transport mock
//! (`MockBrokerListener`/`MockBrokerConn`) are both gated behind feature `std` — this crate's
//! existing `--no-default-features` MCU build (see `justfile`'s `build-no-std`) never compiles
//! them.
//!
//! Per-tick contract (see [`BrokerListener::accept`]/[`BrokerConn::recv`]/[`BrokerConn::send_all`]
//! for the full per-method docs): nonblocking accept (at most one new connection per tick),
//! nonblocking per-client recv (`Bytes(n)` feed / `Closed` orderly close / `WouldBlock` nothing
//! this tick), blocking full-length send (`Sent` / `PeerGone` — a short/failed send means the
//! peer is gone and the caller must close the slot, never retry piecemeal).

mod types;

pub use types::{BrokerConn, BrokerListener, RecvOutcome, SendOutcome};

#[cfg(feature = "std")]
pub use types::{
    MockBrokerConn, MockBrokerListener, MockRecvEvent, SendFault, SendFaultKind, TcpBrokerConn,
    TcpBrokerListener,
};

#[cfg(feature = "std")]
mod impl_tcp_broker_conn;
#[cfg(feature = "std")]
mod impl_tcp_broker_listener;

#[cfg(feature = "std")]
mod impl_mock_broker_conn;
#[cfg(feature = "std")]
mod impl_mock_broker_listener;

#[cfg(all(test, feature = "std"))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;
