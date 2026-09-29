//! **M1 host bring-up proof**: two independent `embassy_net::Stack`s (client + server), each
//! wired to its own [`lib_net::link::NetLink`] endpoint of an in-memory two-peer bridge, run on a real
//! `embassy-executor` `arch-std` executor (its own OS thread — genuinely suspending on `.await`,
//! not a `block_on`-over-blocking-calls side-step per the repo's Abstraction & Platform Seams
//! rule) — and a real `embassy_net::tcp::TcpSocket` connects client -> server and exchanges bytes
//! over it. This proves the whole chain composes: `NetLink` -> [`lib_net::embassy::adapter`] ->
//! `embassy-net-driver-channel` -> `embassy_net::Stack` -> `TcpSocket`.
//!
//! No emulator: this is host-only (feature `host`), run under `cargo test -p lib-net-embassy
//! --features host --test host_bring_up`.
//!
//! `#[embassy_executor::task]` functions must be concrete/non-generic (see `src/host/mod.rs`'s
//! doc), so the two-peer bridge link type and the task wrappers closing over it live here,
//! monomorphized for this one proof, rather than in the library.

#![cfg(feature = "embassy-host")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use embassy_executor::Executor;
use embassy_net::{Ipv4Address, Ipv4Cidr, Stack, StackResources};
use embassy_net_driver::LinkState;
use embassy_net_driver_channel::{RxRunner, State, TxRunner};
use embedded_io_async::Write;
use lib_net::channel::{FlowDir, WireFrame};
use lib_net::embassy::config::{StackConfig, MTU, N_RX, N_TX};
use lib_net::embassy::{host, pump_channel_to_link, pump_link_to_channel};
use lib_net::link::{LinkError, NetLink};
use static_cell::StaticCell;
use std::sync::mpsc as std_mpsc;
use tokio::sync::mpsc as tok_mpsc;

/// One direction of a two-peer in-memory link: `send` enqueues onto this direction's outbound
/// queue, `recv` dequeues this direction's inbound queue. Unlike [`lib_net::link::LoopbackLink`]
/// (which loops a single queue back to itself), a [`DirLink`] pair is a genuine two-party link —
/// the M1 seam under test (`NetLink` -> driver-channel -> `embassy_net::Stack`) needs two
/// independent stacks talking to each other, not one stack talking to itself. Each stack's
/// ingress and egress pump tasks each need their OWN owned [`NetLink`] (`recv`/`send` both take
/// `&mut self`), so a peer's traffic is carried by TWO `DirLink`s — one the ingress task only
/// ever `recv`s from, one the egress task only ever `send`s on — sharing nothing, wired to the
/// SAME pair of channel endpoints as the peer on the other side.
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
            // The egress-only half never calls recv; nothing is ever sent, so nothing ever
            // arrives — Down would be wrong (implies "closed", but this side just isn't wired
            // for ingress), so this pends forever instead, matching "no ingress traffic on this
            // half" rather than "link gone".
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
            None => Ok(()), // the ingress-only half never calls send; no-op if it somehow did
        }
    }
    fn mac(&self) -> [u8; 6] {
        self.mac
    }
}

/// A cross-wired two-peer bridge, split straight into the four [`DirLink`] halves the two peers'
/// ingress/egress tasks need: `(a_ingress, a_egress, b_ingress, b_egress)`. Whatever `a` sends,
/// `b`'s ingress task receives, and vice versa.
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

/// Build one peer's stack over `(ingress_link, egress_link)` and spawn its tasks (net runner,
/// ingress pump, egress pump) on `spawner`. `channel_state`/`stack_resources` are `&'static mut`
/// — the caller promotes them via `static_cell::StaticCell` (the standard embassy pattern; see
/// [`host::build_stack`]'s doc).
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
    // The driver-channel starts LinkState::Down (see embassy-net-driver-channel's `new`); the
    // real Zephyr/W5500 driver would flip this once the PHY reports carrier — here the link is
    // "up" as soon as the bridge exists.
    state_runner.set_link_state(LinkState::Up);

    // embassy-executor 0.10: `#[task]` fns return `Result<SpawnToken, SpawnError>` and
    // `Spawner::spawn` takes the (unwrapped) token and returns `()` — so the unwrap moves INSIDE
    // the `spawn(...)` call (was `.spawn(task(..)).unwrap()` under 0.7's flipped contract).
    spawner.spawn(net_task(parts.net_runner).unwrap());
    spawner.spawn(ingress_task(ingress_link, rx_runner).unwrap());
    spawner.spawn(egress_task(egress_link, tx_runner).unwrap());

    parts.stack
}

/// The whole bring-up + TCP exchange, run as an embassy task on the executor thread. Reports the
/// bytes the server observed back to the test's calling thread via `done`.
#[embassy_executor::task]
async fn scenario(
    spawner: embassy_executor::Spawner,
    client_ingress: DirLink,
    client_egress: DirLink,
    server_ingress: DirLink,
    server_egress: DirLink,
    done: std_mpsc::Sender<Vec<u8>>,
) {
    static CLIENT_STATE: StaticCell<State<MTU, N_RX, N_TX>> = StaticCell::new();
    static SERVER_STATE: StaticCell<State<MTU, N_RX, N_TX>> = StaticCell::new();
    static CLIENT_RESOURCES: StaticCell<StackResources<1>> = StaticCell::new();
    static SERVER_RESOURCES: StaticCell<StackResources<1>> = StaticCell::new();

    let client_config = StackConfig::test_net_1([2, 0, 0, 0, 0, 1]); // 192.0.2.1/24
    let mut server_config = StackConfig::test_net_1([2, 0, 0, 0, 0, 2]);
    server_config.cidr = Ipv4Cidr::new(Ipv4Address::new(192, 0, 2, 2), 24);

    let client_stack = start_peer(
        spawner,
        client_config,
        client_ingress,
        client_egress,
        CLIENT_STATE.init(State::new()),
        CLIENT_RESOURCES.init(StackResources::new()),
        1,
    );
    let server_stack = start_peer(
        spawner,
        server_config,
        server_ingress,
        server_egress,
        SERVER_STATE.init(State::new()),
        SERVER_RESOURCES.init(StackResources::new()),
        2,
    );

    client_stack.wait_config_up().await;
    server_stack.wait_config_up().await;

    static SERVER_RX: StaticCell<[u8; 512]> = StaticCell::new();
    static SERVER_TX: StaticCell<[u8; 512]> = StaticCell::new();
    static CLIENT_RX: StaticCell<[u8; 512]> = StaticCell::new();
    static CLIENT_TX: StaticCell<[u8; 512]> = StaticCell::new();

    let mut server_socket = embassy_net::tcp::TcpSocket::new(
        server_stack,
        SERVER_RX.init([0; 512]),
        SERVER_TX.init([0; 512]),
    );
    let mut client_socket = embassy_net::tcp::TcpSocket::new(
        client_stack,
        CLIENT_RX.init([0; 512]),
        CLIENT_TX.init([0; 512]),
    );

    // Server listens on :7000; client dials 192.0.2.2:7000. Accept and connect race concurrently
    // (a listening socket must be armed before the peer's SYN can complete the handshake).
    let accept = server_socket.accept(7000);
    let connect = client_socket.connect((Ipv4Address::new(192, 0, 2, 2), 7000));
    let (accept_res, connect_res) = embassy_futures::join::join(accept, connect).await;
    accept_res.expect("server accept");
    connect_res.expect("client connect");

    client_socket
        .write_all(b"hello from the M1 embassy-net spike")
        .await
        .expect("client write");
    client_socket.flush().await.expect("client flush");

    let mut buf = [0u8; 64];
    let n = server_socket.read(&mut buf).await.expect("server read");

    let _ = done.send(buf[..n].to_vec());
}

#[test]
fn tcp_connects_and_exchanges_bytes_over_the_embassy_net_adapter() {
    let (client_ingress, client_egress, server_ingress, server_egress) =
        peer_bridge([2, 0, 0, 0, 0, 1], [2, 0, 0, 0, 0, 2]);

    let (done_tx, done_rx) = std_mpsc::channel::<Vec<u8>>();

    // A real embassy-executor `arch-std` executor on its own OS thread — the tasks above
    // genuinely suspend on `.await` (tokio mpsc recv, driver-channel zerocopy_channel waits),
    // not a `block_on` wrapping blocking calls.
    let _executor_thread = std::thread::spawn(move || {
        let executor = Box::leak(Box::new(Executor::new()));
        executor.run(|spawner| {
            spawner.spawn(
                scenario(
                    spawner,
                    client_ingress,
                    client_egress,
                    server_ingress,
                    server_egress,
                    done_tx,
                )
                .unwrap(),
            );
        });
    });

    let received = done_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("scenario completed within timeout");
    assert_eq!(received, b"hello from the M1 embassy-net spike");

    // `Executor::run` never returns (an embedded executor's normal shape — see the repo's
    // Embedded/Firmware rules on `impl-app`'s own Zephyr entry point behaving the same way), so
    // the test doesn't join the thread — the process ending at test completion is the teardown.
}
