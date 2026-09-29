//! `lib-link` — the owned network layer: L2 driver + L3/L4 stack.
//!
//! The bottom is the **driver**: a [`NetLink`] that moves raw Ethernet frames to/from the hardware.
//! Choosing the driver is choosing a `NetLink` impl — nothing above L2 changes:
//!
//! * **host** — `host::HostLink`, Linux `pnet`/AF_PACKET (keeps the mature kernel NIC driver for
//!   L2, bypasses the kernel TCP/IP stack so we own everything above),
//! * **Zephyr** — the board's Ethernet driver,
//! * **MCU** — a W5500 / ENC28J60 SPI Ethernet (a small owned driver),
//! * **[`LoopbackLink`]** — an in-memory link for tests / bring-up.
//!
//! Above L2 is the **owned stack** ([`stack::NetStack`]): smoltcp (no_std, no-alloc) driven by the
//! [`WireFrame`] channels via [`device::ChannelDevice`], exposing **sockets**. The driver's frames
//! arrive on the internal channel ([`pump_ingress`]/[`pump_egress`] do the wiring), flow up through
//! the device into smoltcp transparently, and the app just reads/writes a TCP connection — it never
//! touches a frame. The channel stays the delineation boundary *and* bridges the async driver to
//! smoltcp's synchronous poll.

use crate::channel::{FlowDir, Receiver, Sender, WireFrame};

/// The Linux host L2 driver (the RPi link) over `pnet`/AF_PACKET. Feature `host`.
#[cfg(feature = "link-host")]
pub mod host;

/// The device-hosted MQTT broker pump's transport seam (`BrokerListener`/`BrokerConn`) — host
/// (`std::net`, feature `std`) + mock (feature `std`) impls live here; the Zephyr `sz_sock_*`
/// impl lives in `impl-app` (feature `zephyr`), since this crate stays `no_std`-clean by default.
pub mod broker_transport;
/// The smoltcp [`phy::Device`](smoltcp::phy::Device) bridge over the wire-byte channels.
pub mod device;
/// An owned MQTT 3.1.1 client — wire encoders/parsers, pure and no_std-safe. See
/// `test-e2e/tests/owned_mqtt.rs` (moved from this crate in #99). The two-stack
/// `LoopbackBridge` test helper used by that test's loopback composition case lives in the
/// consolidated `test-kit` dev-dependency crate.
pub mod mqtt_client;
/// A minimal, caller-driven MQTT 3.1.1 **broker core**: the device hosting the server side, so
/// unmodified stock MQTT 3.1.1 clients connect *to* it. QoS 0 only, `#`/`+` topic filters, bounded
/// client table, fixed-slot retained messages. Pure and no_std-safe (byte-in/byte-out, no I/O of
/// its own), built on the same shared `mqtt_wire` primitives as [`mqtt_client`]. See
/// `test-e2e/tests/broker_stock_client.rs` for the stock-client (`rumqttc`) interop proof.
pub mod mqtt_server;
/// The owned MQTT 3.1.1 client *session*: the `CONNECT -> CONNACK -> PUBLISH` connection-flow state
/// machine (with bounded keep-alive) over the [`mqtt_client`] encoders. Pure and no_std-safe;
/// transport-agnostic (rides a raw [`NetStack`] socket or a `TlsStage`). See
/// `lib-link/tests/owned_broker_e2e.rs`.
pub mod mqtt_session;
/// Shared MQTT 3.1.1 wire primitives (fixed header, Remaining Length varint, UTF-8 string fields)
/// used by both [`mqtt_client`] and [`mqtt_server`]. Not part of the public API surface.
mod mqtt_wire;
/// The owned L3+ stack (smoltcp) exposing sockets over any [`NetLink`] driver.
pub mod stack;
/// A TLS record-layer stage over a `NetStack` TCP socket (rustls 0.22). Feature `tls`. See
/// `lib-link/tests/tls_stage.rs`.
#[cfg(feature = "tls")]
pub mod tls;
/// The server identity seam `tls::TlsStage` takes its certificate/key material through, backed by
/// `lib-security`'s `KeyStore`/custody model. Feature `tls`.
#[cfg(feature = "tls")]
pub mod tls_identity;

mod impl_loopback_link;
mod types;

pub use broker_transport::{BrokerConn, BrokerListener, RecvOutcome, SendOutcome};
pub use device::ChannelDevice;
pub use mqtt_server::{BrokerCore, BrokerError};
pub use stack::NetStack;
pub use types::{LinkError, LoopbackLink, NetLink};

/// Ingress pump: move frames from the link onto the wire-byte channel, forever. Stops when the link
/// goes [`LinkError::Down`] or the channel closes. Transient link errors are skipped.
pub async fn pump_ingress<L, Tx>(mut link: L, mut tx: Tx)
where
    L: NetLink,
    Tx: Sender<Item = WireFrame>,
{
    loop {
        let mut frame = WireFrame::empty(FlowDir::Ingress);
        match link.recv(&mut frame).await {
            Ok(()) => {
                if tx.send(frame).await.is_err() {
                    break; // downstream closed
                }
            }
            Err(LinkError::Down) => break,
            Err(_) => {} // transient (overrun/device) — keep receiving
        }
    }
}

/// Egress pump: move frames from the wire-byte channel to the link, forever. Stops when the channel
/// closes. A failed transmit is dropped (the stack retransmits); a downed link ends the pump.
pub async fn pump_egress<L, Rx>(mut link: L, mut rx: Rx)
where
    L: NetLink,
    Rx: Receiver<Item = WireFrame>,
{
    while let Some(frame) = rx.recv().await {
        if let Err(LinkError::Down) = link.send(&frame).await {
            break;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::channel::{Closed, TryRecvError, TrySendError};

    // Minimal tokio-mpsc adapters to the channel traits, for driving the async pumps in tests.
    struct MpscTx<T>(tokio::sync::mpsc::Sender<T>);
    impl<T> Sender for MpscTx<T> {
        type Item = T;
        fn try_send(&mut self, item: T) -> Result<(), TrySendError<T>> {
            self.0.try_send(item).map_err(|e| match e {
                tokio::sync::mpsc::error::TrySendError::Full(i) => TrySendError::Full(i),
                tokio::sync::mpsc::error::TrySendError::Closed(i) => TrySendError::Closed(i),
            })
        }
        async fn send(&mut self, item: T) -> Result<(), Closed> {
            self.0.send(item).await.map_err(|_| Closed)
        }
    }
    struct MpscRx<T>(tokio::sync::mpsc::Receiver<T>);
    impl<T> Receiver for MpscRx<T> {
        type Item = T;
        fn try_recv(&mut self) -> Result<T, TryRecvError> {
            self.0.try_recv().map_err(|e| match e {
                tokio::sync::mpsc::error::TryRecvError::Empty => TryRecvError::Empty,
                tokio::sync::mpsc::error::TryRecvError::Disconnected => TryRecvError::Closed,
            })
        }
        async fn recv(&mut self) -> Option<T> {
            self.0.recv().await
        }
    }

    #[tokio::test]
    async fn ingress_pump_drains_link_to_channel() {
        let mut link: LoopbackLink<8> = LoopbackLink::new([2, 0, 0, 0, 0, 1]);
        link.preload(b"frame-one").unwrap();
        link.preload(b"frame-two").unwrap();

        let (tx, rx) = tokio::sync::mpsc::channel::<WireFrame>(8);
        // pump runs until the link reports Down (queue drained), then returns.
        pump_ingress(link, MpscTx(tx)).await;

        let mut rx = MpscRx(rx);
        assert_eq!(rx.try_recv().unwrap().data, b"frame-one");
        assert_eq!(rx.try_recv().unwrap().data, b"frame-two");
        assert_eq!(rx.try_recv(), Err(TryRecvError::Closed)); // sender dropped after pump ended
    }

    #[tokio::test]
    async fn netlink_round_trips_a_frame() {
        let mut link: LoopbackLink<4> = LoopbackLink::new([2, 0, 0, 0, 0, 2]);
        let f = WireFrame::new(FlowDir::Egress, b"hello-wire").unwrap();
        link.send(&f).await.unwrap();

        let mut out = WireFrame::empty(FlowDir::Ingress);
        link.recv(&mut out).await.unwrap();
        assert_eq!(out.dir, FlowDir::Ingress);
        assert_eq!(out.data, b"hello-wire");
        assert_eq!(link.mac(), [2, 0, 0, 0, 0, 2]);
        // Drained → Down.
        assert_eq!(link.recv(&mut out).await, Err(LinkError::Down));
    }
}
