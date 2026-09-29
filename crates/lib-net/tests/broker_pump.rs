//! **Deliverable 3 host e2e test**: extends M1's `host_bring_up.rs` two-peer harness (real
//! `embassy-executor` `arch-std` executor on its own OS thread, `DirLink`/`peer_bridge`/
//! `start_peer` two-peer bridge) with a broker peer running [`lib_net::embassy::broker_slot_cycle`]
//! over a [`lib_net::link::BrokerCore`], and a client peer whose `TcpSocket` connects, sends CONNECT then
//! SUBSCRIBE, and reads back CONNACK + SUBACK + a ~1.6 KB retained PUBLISH byte-exact — proving the
//! whole chain (`NetLink` -> `lib_net::embassy::adapter` -> `embassy_net::Stack` -> `TcpSocket` ->
//! [`lib_net::embassy::pump`] -> `BrokerCore`) composes, including the large-payload streamed-write
//! path across a BOUNDED `TcpSocket` tx buffer (deliberately sized smaller than the payload, forcing
//! [`lib_net::embassy::drain_and_write`]'s partial-write resumption to actually exercise itself, not
//! just complete in one shot).
//!
//! No emulator: host-only (feature `host`), run under `cargo test -p lib-net-embassy --features
//! host --test broker_pump`.

#![cfg(feature = "embassy-host")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use embassy_executor::Executor;
use embassy_net::{Ipv4Address, Ipv4Cidr, Stack, StackResources};
use embassy_net_driver::LinkState;
use embassy_net_driver_channel::{RxRunner, State, TxRunner};
use lib_net::channel::{FlowDir, WireFrame};
use lib_net::embassy::config::{StackConfig, MTU, N_RX, N_TX};
use lib_net::embassy::{broker_slot_cycle, host, pump_channel_to_link, pump_link_to_channel};
use lib_net::link::mqtt_client::{encode_connect, parse_connack, ConnectOptions};
use lib_net::link::{BrokerCore, LinkError, NetLink};
use static_cell::StaticCell;
use std::sync::mpsc as std_mpsc;
use tokio::sync::mpsc as tok_mpsc;

const TOPIC: &str = "tms/dev01/big";
const PAYLOAD_LEN: usize = 1600;

/// One direction of a two-peer in-memory link — identical shape to `tests/host_bring_up.rs`'s
/// `DirLink` (see that file's doc for why a two-party bridge needs two owned halves per peer).
struct DirLink {
    inbound: Option<tok_mpsc::Receiver<WireFrame>>,
    outbound: Option<tok_mpsc::Sender<WireFrame>>,
    mac: [u8; 6],
}

impl NetLink for DirLink {
    async fn recv(&mut self, out: &mut WireFrame) -> Result<(), LinkError> {
        match self.inbound.as_mut() {
            Some(rx) => match rx.recv().await {
                Some(frame) => {
                    *out = frame;
                    out.dir = FlowDir::Ingress;
                    Ok(())
                }
                None => Err(LinkError::Down),
            },
            None => std::future::pending().await,
        }
    }
    async fn send(&mut self, frame: &WireFrame) -> Result<(), LinkError> {
        match self.outbound.as_mut() {
            Some(tx) => {
                let mut owned = WireFrame::empty(FlowDir::Ingress);
                owned
                    .data
                    .extend_from_slice(&frame.data)
                    .map_err(|_| LinkError::Overrun)?;
                tx.send(owned).await.map_err(|_| LinkError::Down)
            }
            None => Ok(()),
        }
    }
    fn mac(&self) -> [u8; 6] {
        self.mac
    }
}

fn peer_bridge(mac_a: [u8; 6], mac_b: [u8; 6]) -> (DirLink, DirLink, DirLink, DirLink) {
    let (a_to_b, b_inbound) = tok_mpsc::channel(16);
    let (b_to_a, a_inbound) = tok_mpsc::channel(16);
    let a_ingress = DirLink {
        inbound: Some(a_inbound),
        outbound: None,
        mac: mac_a,
    };
    let a_egress = DirLink {
        inbound: None,
        outbound: Some(a_to_b),
        mac: mac_a,
    };
    let b_ingress = DirLink {
        inbound: Some(b_inbound),
        outbound: None,
        mac: mac_b,
    };
    let b_egress = DirLink {
        inbound: None,
        outbound: Some(b_to_a),
        mac: mac_b,
    };
    (a_ingress, a_egress, b_ingress, b_egress)
}

#[embassy_executor::task(pool_size = 2)]
async fn net_task(
    mut runner: embassy_net::Runner<'static, embassy_net_driver_channel::Device<'static, MTU>>,
) {
    runner.run().await
}

#[embassy_executor::task(pool_size = 2)]
async fn ingress_task(link: DirLink, rx: RxRunner<'static, MTU>) {
    pump_link_to_channel::<_, MTU>(link, rx).await;
}

#[embassy_executor::task(pool_size = 2)]
async fn egress_task(link: DirLink, tx: TxRunner<'static, MTU>) {
    pump_channel_to_link::<_, MTU>(link, tx).await;
}

fn start_peer(
    spawner: embassy_executor::Spawner,
    config: StackConfig,
    ingress_link: DirLink,
    egress_link: DirLink,
    channel_state: &'static mut State<MTU, N_RX, N_TX>,
    stack_resources: &'static mut StackResources<1>,
    seed: u64,
) -> Stack<'static> {
    let parts = host::build_stack(&config, channel_state, stack_resources, seed);
    let (state_runner, rx_runner, tx_runner) = parts.channel_runner.split();
    state_runner.set_link_state(LinkState::Up);

    // embassy-executor 0.10: unwrap the SpawnToken INSIDE `spawn(...)` (see host_bring_up.rs's
    // matching note) — `spawn` itself now returns `()`.
    spawner.spawn(net_task(parts.net_runner).unwrap());
    spawner.spawn(ingress_task(ingress_link, rx_runner).unwrap());
    spawner.spawn(egress_task(egress_link, tx_runner).unwrap());

    parts.stack
}

/// The broker peer's task: owns a `BrokerCore<4>` and runs [`broker_slot_cycle`] on slot 0 in a
/// loop (re-listening after each connection — see that fn's doc), rendering `TOPIC`'s payload from
/// a fixed patterned buffer whenever the client's SUBSCRIBE triggers the retained-replay path.
/// `TOPIC` is published (retained) BEFORE the accept loop starts — mirroring
/// `mqtt_server.rs`'s own `retained_topic_replays_via_render_to_a_late_subscriber_with_retain_flag_set`
/// test: `publish_topic`'s active-topic bookkeeping lives in `BrokerCore` itself, independent of any
/// client slot, so publishing before anyone has connected and relying on the subsequent SUBSCRIBE's
/// retained-replay (MQTT 3.1.1 section 3.3.1.3) to deliver it is deterministic — no timing race
/// against the live connection needed.
///
/// The `TcpSocket`'s tx buffer (256 B) is deliberately far smaller than `PAYLOAD_LEN` (1600 B) plus
/// MQTT framing, forcing embassy-net's own internal segmentation AND
/// [`lib_net::embassy::drain_and_write`]'s `write_all`-based resumption to actually engage across
/// many partial writes — this is the deliverable's whole point (streaming a large payload across a
/// bounded transport window without desync).
#[embassy_executor::task]
async fn broker_task(stack: Stack<'static>) {
    static RX: StaticCell<[u8; 512]> = StaticCell::new();
    static TX: StaticCell<[u8; 256]> = StaticCell::new(); // deliberately << PAYLOAD_LEN + framing
    let mut socket = embassy_net::tcp::TcpSocket::new(stack, RX.init([0; 512]), TX.init([0; 256]));

    let payload: Vec<u8> = (0..PAYLOAD_LEN).map(|i| (i % 251) as u8).collect();
    let mut broker: BrokerCore<4> = BrokerCore::new();
    broker.publish_topic(TOPIC, true);

    let mut rx_buf = [0u8; 2048];
    let mut out = vec![0u8; BrokerCore::<4>::min_out_len(PAYLOAD_LEN)];

    loop {
        let payload_ref = &payload;
        let mut render = move |topic: &str, buf: &mut [u8]| -> Option<usize> {
            if topic == TOPIC {
                buf.get_mut(..PAYLOAD_LEN)?.copy_from_slice(payload_ref);
                Some(PAYLOAD_LEN)
            } else {
                None
            }
        };

        broker_slot_cycle::<_, _, _, _, 4>(
            &mut broker,
            0,
            &mut socket,
            &mut rx_buf,
            &mut out,
            &mut render,
            async |s: &mut embassy_net::tcp::TcpSocket<'_>| s.accept(1883).await.is_ok(),
            async |s: &mut embassy_net::tcp::TcpSocket<'_>| {
                s.close();
                s.abort();
                let _ = s.flush().await;
            },
        )
        .await;
    }
}

/// Bring-up + MQTT exchange, run as an embassy task. Reports the client's parsed
/// CONNACK/SUBACK-presence + the received PUBLISH's topic/payload bytes back to the test thread.
#[embassy_executor::task]
async fn scenario(
    spawner: embassy_executor::Spawner,
    client_ingress: DirLink,
    client_egress: DirLink,
    broker_ingress: DirLink,
    broker_egress: DirLink,
    done: std_mpsc::Sender<(u8, Vec<u8>)>, // (connack_return_code, publish_payload)
) {
    static CLIENT_STATE: StaticCell<State<MTU, N_RX, N_TX>> = StaticCell::new();
    static BROKER_STATE: StaticCell<State<MTU, N_RX, N_TX>> = StaticCell::new();
    static CLIENT_RESOURCES: StaticCell<StackResources<1>> = StaticCell::new();
    static BROKER_RESOURCES: StaticCell<StackResources<1>> = StaticCell::new();

    let client_config = StackConfig::test_net_1([2, 0, 0, 1, 0, 1]);
    let mut broker_config = StackConfig::test_net_1([2, 0, 0, 1, 0, 2]);
    broker_config.cidr = Ipv4Cidr::new(Ipv4Address::new(192, 0, 2, 2), 24);

    let client_stack = start_peer(
        spawner,
        client_config,
        client_ingress,
        client_egress,
        CLIENT_STATE.init(State::new()),
        CLIENT_RESOURCES.init(StackResources::new()),
        1,
    );
    let broker_stack = start_peer(
        spawner,
        broker_config,
        broker_ingress,
        broker_egress,
        BROKER_STATE.init(State::new()),
        BROKER_RESOURCES.init(StackResources::new()),
        2,
    );

    client_stack.wait_config_up().await;
    broker_stack.wait_config_up().await;

    spawner.spawn(broker_task(broker_stack).unwrap());

    static CLIENT_RX: StaticCell<[u8; 512]> = StaticCell::new();
    static CLIENT_TX: StaticCell<[u8; 512]> = StaticCell::new();
    let mut client_socket = embassy_net::tcp::TcpSocket::new(
        client_stack,
        CLIENT_RX.init([0; 512]),
        CLIENT_TX.init([0; 512]),
    );

    // Give the broker task a moment to reach `accept` before the client dials (the broker task is
    // spawned just above; embassy's cooperative scheduler runs it up to its first suspend point —
    // `accept` — before this task is polled again, but an explicit yield keeps the ordering robust
    // against scheduler changes rather than relying on spawn-order alone).
    embassy_futures::yield_now().await;

    client_socket
        .connect((Ipv4Address::new(192, 0, 2, 2), 1883))
        .await
        .expect("client connect");

    // CONNECT
    let mut buf = [0u8; 128];
    let n = encode_connect(
        &ConnectOptions {
            client_id: "e2e-client",
            keep_alive_secs: 60,
            clean_session: true,
            username: None,
            password: None,
        },
        &mut buf,
    )
    .expect("encode CONNECT");
    embedded_io_async::Write::write_all(&mut client_socket, &buf[..n])
        .await
        .expect("send CONNECT");

    // Read CONNACK (exactly 4 bytes for this broker's CONNACK — see mqtt_server.rs::stage_connack).
    let mut connack_buf = [0u8; 4];
    embedded_io_async::Read::read_exact(&mut client_socket, &mut connack_buf)
        .await
        .expect("read CONNACK");
    let ack = parse_connack(&connack_buf).expect("parse CONNACK");

    // SUBSCRIBE (hand-rolled — lib-link's mqtt_wire module is private; see
    // crates/lib-link/src/broker_transport/tests.rs's private encode_subscribe for the same byte
    // layout this reimplements: fixed header 0x82, remaining-length varint, packet id, then one
    // length-prefixed filter string + requested-QoS byte per filter).
    let sub = encode_subscribe(1, TOPIC);
    embedded_io_async::Write::write_all(&mut client_socket, &sub)
        .await
        .expect("send SUBSCRIBE");

    // Read SUBACK: fixed header (2 bytes: type + remaining-length, remaining-length is 3 for one
    // filter here since 1600 < 128 never applies to a 3-byte remaining length) + packet id (2) + 1
    // grant byte = 5 bytes total for a single-filter SUBSCRIBE.
    let mut suback_buf = [0u8; 5];
    embedded_io_async::Read::read_exact(&mut client_socket, &mut suback_buf)
        .await
        .expect("read SUBACK");
    assert_eq!(suback_buf[0], 0x90, "SUBACK packet type");

    // Read the retained PUBLISH: fixed header (up to 5 bytes) + 2-byte topic length-prefix + topic
    // + PAYLOAD_LEN payload bytes. Parse just enough to locate the payload region (skip fixed
    // header via the Remaining Length varint, then the topic length-prefix) rather than a full
    // MQTT parser, per the deliverable's own scope note.
    let mut header_buf = [0u8; 1];
    embedded_io_async::Read::read_exact(&mut client_socket, &mut header_buf)
        .await
        .expect("read PUBLISH fixed-header byte 1");
    assert_eq!(header_buf[0] & 0xF0, 0x30, "PUBLISH packet type");
    assert_eq!(header_buf[0] & 0x01, 0x01, "RETAIN flag set on replay");

    let mut remaining_length = 0usize;
    let mut shift = 0u32;
    loop {
        let mut b = [0u8; 1];
        embedded_io_async::Read::read_exact(&mut client_socket, &mut b)
            .await
            .expect("read Remaining Length byte");
        remaining_length |= ((b[0] & 0x7F) as usize) << shift;
        if b[0] & 0x80 == 0 {
            break;
        }
        shift += 7;
    }

    let mut topic_len_buf = [0u8; 2];
    embedded_io_async::Read::read_exact(&mut client_socket, &mut topic_len_buf)
        .await
        .expect("read topic length prefix");
    let topic_len = u16::from_be_bytes(topic_len_buf) as usize;
    let mut topic_buf = vec![0u8; topic_len];
    embedded_io_async::Read::read_exact(&mut client_socket, &mut topic_buf)
        .await
        .expect("read topic bytes");
    assert_eq!(topic_buf, TOPIC.as_bytes(), "PUBLISH topic matches");

    let payload_len = remaining_length - 2 - topic_len;
    let mut payload_buf = vec![0u8; payload_len];
    embedded_io_async::Read::read_exact(&mut client_socket, &mut payload_buf)
        .await
        .expect("read PUBLISH payload");

    let _ = done.send((ack.return_code, payload_buf));
}

fn encode_subscribe(packet_id: u16, filter: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    let remaining = 2 + 2 + filter.len() + 1;
    buf.push(0x82); // SUBSCRIBE, reserved flags 0010
    let mut rl = remaining;
    loop {
        let mut byte = (rl % 128) as u8;
        rl /= 128;
        if rl > 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if rl == 0 {
            break;
        }
    }
    buf.extend_from_slice(&packet_id.to_be_bytes());
    buf.extend_from_slice(&(filter.len() as u16).to_be_bytes());
    buf.extend_from_slice(filter.as_bytes());
    buf.push(0x00); // requested QoS 0
    buf
}

#[test]
fn client_receives_connack_suback_and_large_retained_publish_byte_exact() {
    let (client_ingress, client_egress, broker_ingress, broker_egress) =
        peer_bridge([2, 0, 0, 1, 0, 1], [2, 0, 0, 1, 0, 2]);

    let (done_tx, done_rx) = std_mpsc::channel::<(u8, Vec<u8>)>();

    let _executor_thread = std::thread::spawn(move || {
        let executor = Box::leak(Box::new(Executor::new()));
        executor.run(|spawner| {
            spawner.spawn(
                scenario(
                    spawner,
                    client_ingress,
                    client_egress,
                    broker_ingress,
                    broker_egress,
                    done_tx,
                )
                .unwrap(),
            );
        });
    });

    let (connack_rc, payload) = done_rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("scenario completed within timeout");

    assert_eq!(connack_rc, 0, "CONNECT must be accepted");
    let expected: Vec<u8> = (0..PAYLOAD_LEN).map(|i| (i % 251) as u8).collect();
    assert_eq!(payload, expected, "PUBLISH payload must be byte-exact");
}
