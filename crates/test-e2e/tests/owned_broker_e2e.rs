//! The owned MQTT client, end to end, against a **real rumqttd broker**.
//!
//! `owned_mqtt.rs` proves the owned client composes with the `TlsStage` in a pure two-`NetStack`
//! loopback (client `TlsStage` <-> server `TlsStage`); this file drives the owned
//! [`MqttSession`](lib_net::link::mqtt_session::MqttSession) all the way through to a genuine,
//! in-process `rumqttd` broker and asserts a `rumqttc` subscriber receives the byte-exact topic and
//! payload the owned client published — the same "broker actually received this" bar the
//! `lib-mqtt` broker tests hold, but reached over the *owned* net stack rather than an OS TCP
//! socket.
//!
//! ## How the owned stack reaches an OS-socket broker: the smoltcp gateway
//!
//! The owned client speaks TCP/IP as raw Ethernet frames over the in-memory [`LoopbackBridge`]; a
//! real `rumqttd` listens on an OS `127.0.0.1` socket. A **gateway** bridges the two: a second
//! `NetStack` accepts the owned client's smoltcp TCP connection and splices its application bytes,
//! byte for byte, onto a real [`std::net::TcpStream`] connected to the broker (and back). The
//! gateway is transport-agnostic — it moves opaque TCP payload — so it carries plaintext MQTT and
//! TLS records identically. That is the whole point: the broker receives exactly the bytes the
//! owned `MqttSession`/`TlsStage` produced.
//!
//! ## Two broker paths (see each test)
//!
//! * [`owned_client_publishes_plaintext_to_real_rumqttd`] — the owned `MqttSession` over a plaintext
//!   smoltcp socket, spliced to a plaintext `rumqttd` listener. This is the unambiguous
//!   "broker received this exact payload" assertion.
//! * [`owned_client_publishes_over_tls_to_real_rumqttd`] (feature `tls`) — genuine end-to-end TLS:
//!   the owned client's `TlsStage` handshakes with `rumqttd`'s **own** TLS listener (configured to
//!   present the same bring-up certificate the client trusts), the sealed PUBLISH is decrypted by
//!   `rumqttd`, and a plaintext `rumqttc` subscriber on a second listener of the same broker sees
//!   the exact topic/payload. TLS is terminated by `rumqttd` itself, not by a proxy.
//!
//! Both are driven by the crate's bounded, mock-millisecond poll loop (`now_ms += 10`, the same
//! clock `NetStack::poll` takes) — never a wall-clock keep-alive sleep. A tiny idle yield lets the
//! real broker/subscriber threads make wall-clock progress between polls; the loop is still bounded
//! by a fixed poll budget.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{HashMap, VecDeque};
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::Duration;

use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};

use lib_net::link::mqtt_client::ConnectOptions;
use lib_net::link::mqtt_session::MqttSession;
use lib_net::link::stack::SocketHandle;
use lib_net::link::NetStack;
use smoltcp::iface::SocketStorage;
use smoltcp::socket::tcp::State;
use smoltcp::wire::{IpAddress, IpEndpoint, Ipv4Address, Ipv4Cidr};
use test_kit::LoopbackBridge;

const CLIENT_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x05];
const GATEWAY_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x06];
const CLIENT_IP: Ipv4Address = Ipv4Address::new(10, 0, 3, 1);
const GATEWAY_IP: Ipv4Address = Ipv4Address::new(10, 0, 3, 2);
/// The smoltcp-side port the gateway listens on (distinct from the broker's OS port).
const GATEWAY_PORT: u16 = 8883;
const CLIENT_PORT: u16 = 55003;

const TOPIC: &str = "sentient/dev01/tms";
const PAYLOAD: &[u8] = b"{\"v\":1,\"src\":\"owned\"}";

/// Bounded poll budget for the owned-client flow. Generous because the real broker/subscriber run
/// on wall-clock threads reached through the gateway's OS socket; still a hard, finite cap.
const MAX_POLLS: usize = 8000;
/// Bounded budget for the two smoltcp stacks' own TCP handshake (pure mock-clock, no OS socket).
const MAX_HANDSHAKE_POLLS: usize = 1000;

/// Bounded retries for the gateway→broker uplink connect (20 ms apart ≈ 2 s), covering the window
/// where rumqttd's TCP bind hasn't started accepting yet on a starved runner (#280).
const GATEWAY_CONNECT_RETRIES: usize = 100;

/// Reserve a free ephemeral `127.0.0.1` port (bind `:0`, read it back, drop the listener) — the
/// same collision-avoidance the `lib-mqtt` broker tests use so concurrent `cargo test` runs
/// never fight over a fixed port.
fn reserve_ephemeral_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind an ephemeral port")
        .local_addr()
        .expect("read back the OS-assigned port")
        .port()
}

/// A `rumqttd` TCP listener to open: a name, an address, and optional TLS material.
struct Listener {
    name: String,
    addr: SocketAddr,
    tls: Option<rumqttd::TlsConfig>,
}

/// Boot a real in-process `rumqttd` broker with the given listeners (mirrors the minimal-config
/// construction in `lib-mqtt::broker`, but built here so the test controls the listener set and
/// TLS). Returns the broker thread handle (kept alive for the test's duration).
fn start_broker(listeners: Vec<Listener>) -> JoinHandle<()> {
    let mut v4 = HashMap::new();
    for l in listeners {
        v4.insert(
            l.name.clone(),
            rumqttd::ServerSettings {
                name: l.name,
                listen: l.addr,
                tls: l.tls,
                next_connection_delay_ms: 0,
                connections: rumqttd::ConnectionSettings {
                    connection_timeout_ms: 10_000,
                    max_payload_size: 1024 * 1024,
                    max_inflight_count: 100,
                    auth: None,
                    external_auth: None,
                    dynamic_filters: false,
                },
            },
        );
    }

    let config = rumqttd::Config {
        id: 0,
        router: rumqttd::RouterConfig {
            max_connections: 1000,
            max_outgoing_packet_count: 1000,
            max_segment_size: 65536,
            max_segment_count: 100,
            custom_segment: None,
            initialized_filters: None,
            shared_subscriptions_strategy: Default::default(),
        },
        v4: Some(v4),
        v5: None,
        ws: None,
        cluster: None,
        console: None,
        bridge: None,
        prometheus: None,
        metrics: None,
    };

    let mut broker = rumqttd::Broker::new(config);
    std::thread::spawn(move || {
        let _ = broker.start();
    })
}

/// A `rumqttc` subscriber running its own tokio runtime on a dedicated thread. Signals `ready` once
/// its SUBACK lands, and forwards every received `(topic, payload)` on `msgs`.
struct Subscriber {
    ready: Receiver<()>,
    msgs: Receiver<(String, Vec<u8>)>,
    _thread: JoinHandle<()>,
}

/// Connect a plaintext `rumqttc` subscriber to `port`, subscribe to `filter`, and pump its
/// eventloop forever (the thread is detached; the test process reaps it on exit).
fn spawn_subscriber(port: u16, filter: &str) -> Subscriber {
    let (ready_tx, ready) = mpsc::channel();
    let (msg_tx, msgs) = mpsc::channel();
    let filter = filter.to_string();
    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("subscriber tokio runtime");
        rt.block_on(async move {
            let mut opts = MqttOptions::new("sentient-owned-sub", "127.0.0.1", port);
            opts.set_keep_alive(Duration::from_secs(5));
            let (client, mut eventloop) = AsyncClient::new(opts, 10);
            client
                .subscribe(&filter, QoS::AtLeastOnce)
                .await
                .expect("subscribe");
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Incoming::SubAck(_))) => {
                        let _ = ready_tx.send(());
                    }
                    Ok(Event::Incoming(Incoming::Publish(p))) => {
                        let _ = msg_tx.send((p.topic.clone(), p.payload.to_vec()));
                    }
                    Ok(_) => {}
                    // The broker may not be up for the very first polls, or shuts down at test end;
                    // back off rather than spin.
                    Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
                }
            }
        });
    });
    Subscriber {
        ready,
        msgs,
        _thread: thread,
    }
}

/// Splice one poll's worth of application bytes between the gateway's smoltcp TCP socket and the
/// real `TcpStream` to the broker, in both directions. Returns whether any byte moved (so the caller
/// can idle-yield when nothing did). Transport-agnostic: it never inspects the payload, so plaintext
/// MQTT and TLS records ride it identically.
fn pump_gateway(
    stack: &mut NetStack<'_>,
    handle: SocketHandle,
    tcp: &mut TcpStream,
    to_broker: &mut VecDeque<u8>,
    from_broker: &mut VecDeque<u8>,
) -> bool {
    let mut progress = false;
    let mut buf = [0u8; 4096];

    // smoltcp socket -> to_broker staging
    loop {
        let sock = stack.tcp(handle);
        if !sock.can_recv() {
            break;
        }
        match sock.recv_slice(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                to_broker.extend(&buf[..n]);
                progress = true;
            }
            Err(_) => break,
        }
    }

    // to_broker staging -> real broker socket
    while !to_broker.is_empty() {
        let front = to_broker.as_slices().0;
        match tcp.write(front) {
            Ok(0) => break,
            Ok(n) => {
                to_broker.drain(..n);
                progress = true;
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => break,
            Err(_) => break,
        }
    }

    // real broker socket -> from_broker staging
    loop {
        match tcp.read(&mut buf) {
            Ok(0) => break, // EOF
            Ok(n) => {
                from_broker.extend(&buf[..n]);
                progress = true;
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => break,
            Err(_) => break,
        }
    }

    // from_broker staging -> smoltcp socket
    loop {
        let sock = stack.tcp(handle);
        if from_broker.is_empty() || !sock.can_send() {
            break;
        }
        let front = from_broker.as_slices().0;
        match sock.send_slice(front) {
            Ok(0) => break,
            Ok(n) => {
                from_broker.drain(..n);
                progress = true;
            }
            Err(_) => break,
        }
    }

    progress
}

/// Build the owned-client `NetStack` and the gateway `NetStack` cross-wired over a `LoopbackBridge`,
/// complete their TCP handshake (mock clock), and open a nonblocking `TcpStream` from the gateway to
/// the broker at `broker_port`. Returns everything the flow loop needs. The socket buffers and
/// storage are caller-owned slices (passed in) to satisfy smoltcp's lifetimes.
#[allow(clippy::too_many_arguments)]
fn establish<'a>(
    dev_client: &mut lib_net::link::ChannelDevice<test_kit::BridgeEnd, test_kit::BridgeEnd>,
    dev_gateway: &mut lib_net::link::ChannelDevice<test_kit::BridgeEnd, test_kit::BridgeEnd>,
    client_sockets: &'a mut [SocketStorage<'a>],
    gateway_sockets: &'a mut [SocketStorage<'a>],
    client_rx: &'a mut [u8],
    client_tx: &'a mut [u8],
    gateway_rx: &'a mut [u8],
    gateway_tx: &'a mut [u8],
    broker_port: u16,
) -> (
    NetStack<'a>,
    SocketHandle,
    NetStack<'a>,
    SocketHandle,
    TcpStream,
    i64,
) {
    let mut client_stack = NetStack::new(
        dev_client,
        CLIENT_MAC,
        Ipv4Cidr::new(CLIENT_IP, 24),
        None,
        client_sockets,
    );
    let mut gateway_stack = NetStack::new(
        dev_gateway,
        GATEWAY_MAC,
        Ipv4Cidr::new(GATEWAY_IP, 24),
        None,
        gateway_sockets,
    );

    let client_handle = client_stack.add_tcp(client_rx, client_tx);
    let gateway_handle = gateway_stack.add_tcp(gateway_rx, gateway_tx);

    gateway_stack
        .tcp(gateway_handle)
        .listen(GATEWAY_PORT)
        .expect("gateway can listen");
    client_stack
        .connect(
            client_handle,
            IpEndpoint::new(IpAddress::Ipv4(GATEWAY_IP), GATEWAY_PORT),
            CLIENT_PORT,
        )
        .expect("client can initiate the connection");

    let mut now_ms: i64 = 0;
    let mut established = false;
    for _ in 0..MAX_HANDSHAKE_POLLS {
        client_stack.poll(dev_client, now_ms);
        gateway_stack.poll(dev_gateway, now_ms);
        if client_stack.tcp(client_handle).state() == State::Established
            && gateway_stack.tcp(gateway_handle).state() == State::Established
        {
            established = true;
            break;
        }
        now_ms += 10;
    }
    assert!(
        established,
        "owned client and gateway should complete a smoltcp TCP handshake"
    );

    // Open the gateway's uplink to the real broker now that the owned client is connected to us.
    // The broker's TCP listener may not be accepting the instant the smoltcp handshake above
    // completes (rumqttd's bind races the test on loaded runners — #280), so retry the connect
    // briefly rather than a one-shot `.expect()` that flakes. First attempt succeeds immediately on
    // the happy path; the bound (~2 s) only matters when the runner is starved.
    let tcp = {
        let mut attempt = 0;
        loop {
            match TcpStream::connect(("127.0.0.1", broker_port)) {
                Ok(s) => break s,
                Err(_) if attempt < GATEWAY_CONNECT_RETRIES => {
                    attempt += 1;
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(e) => panic!("gateway connects to broker after {attempt} retries: {e}"),
            }
        }
    };
    tcp.set_nodelay(true).expect("nodelay");
    tcp.set_nonblocking(true)
        .expect("nonblocking gateway uplink");

    (
        client_stack,
        client_handle,
        gateway_stack,
        gateway_handle,
        tcp,
        now_ms,
    )
}

/// Wait (bounded) for the subscriber's SUBACK so the subscription is live before the owned client
/// publishes (no retained messages here).
fn await_subscription(sub: &Subscriber) {
    sub.ready
        .recv_timeout(Duration::from_secs(10))
        .expect("subscriber's SUBACK should arrive");
}

#[test]
fn owned_client_publishes_plaintext_to_real_rumqttd() {
    let broker_port = reserve_ephemeral_port();
    let _broker = start_broker(vec![Listener {
        name: "plain".to_string(),
        addr: SocketAddr::from(([127, 0, 0, 1], broker_port)),
        tls: None,
    }]);

    let sub = spawn_subscriber(broker_port, "sentient/#");
    await_subscription(&sub);

    let (mut dev_client, mut dev_gateway) = LoopbackBridge::new(1514).split();
    let mut client_sockets: [SocketStorage; 1] = Default::default();
    let mut gateway_sockets: [SocketStorage; 1] = Default::default();
    let mut client_rx = [0u8; 4096];
    let mut client_tx = [0u8; 4096];
    let mut gateway_rx = [0u8; 4096];
    let mut gateway_tx = [0u8; 4096];

    let (mut client_stack, client_handle, mut gateway_stack, gateway_handle, mut tcp, mut now_ms) =
        establish(
            &mut dev_client,
            &mut dev_gateway,
            &mut client_sockets,
            &mut gateway_sockets,
            &mut client_rx,
            &mut client_tx,
            &mut gateway_rx,
            &mut gateway_tx,
            broker_port,
        );

    let opts = ConnectOptions {
        client_id: "sentient-owned-pub",
        keep_alive_secs: 60,
        clean_session: true,
        username: None,
        password: None,
    };
    let mut session = MqttSession::new(opts);

    let mut to_broker: VecDeque<u8> = VecDeque::new();
    let mut from_broker: VecDeque<u8> = VecDeque::new();
    let mut inbuf = [0u8; 4096];
    let mut outbuf = [0u8; 4096];
    let mut published = false;
    let mut got: Option<(String, Vec<u8>)> = None;

    for _ in 0..MAX_POLLS {
        client_stack.poll(&mut dev_client, now_ms);
        gateway_stack.poll(&mut dev_gateway, now_ms);
        let mut progress = pump_gateway(
            &mut gateway_stack,
            gateway_handle,
            &mut tcp,
            &mut to_broker,
            &mut from_broker,
        );

        // Owned client: plaintext MQTT straight over the smoltcp socket.
        let incoming: &[u8] = if client_stack.tcp(client_handle).can_recv() {
            let n = client_stack
                .tcp(client_handle)
                .recv_slice(&mut inbuf)
                .expect("client recv");
            &inbuf[..n]
        } else {
            &[]
        };
        let out_n = session
            .poll(now_ms, incoming, &mut outbuf)
            .expect("session poll");
        if out_n > 0 {
            client_stack
                .tcp(client_handle)
                .send_slice(&outbuf[..out_n])
                .expect("client send");
            progress = true;
        }
        if session.is_connected() && !published {
            session.publish(TOPIC, PAYLOAD).expect("queue PUBLISH");
            published = true;
        }

        if let Ok(msg) = sub.msgs.try_recv() {
            got = Some(msg);
            break;
        }
        if !progress {
            std::thread::sleep(Duration::from_micros(500));
        }
        now_ms += 10;
    }

    let (topic, payload) = got.expect("subscriber should receive the owned client's PUBLISH");
    assert_eq!(topic, TOPIC, "broker delivered the exact topic");
    assert_eq!(payload, PAYLOAD, "broker delivered the exact payload bytes");
    assert!(
        published,
        "the owned client reached the connected state and published"
    );
}

/// Minimal, dependency-free base64 (standard alphabet, padded) — just enough to wrap the bring-up
/// DER as PEM for `rumqttd`'s file-based TLS config, without pulling a base64 crate into the graph.
fn base64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Wrap `der` as a PEM block with the given `label` (64-column base64 body).
fn to_pem(label: &str, der: &[u8]) -> String {
    let b64 = base64_encode(der);
    let mut s = format!("-----BEGIN {label}-----\n");
    for line in b64.as_bytes().chunks(64) {
        s.push_str(std::str::from_utf8(line).expect("base64 is ascii"));
        s.push('\n');
    }
    s.push_str(&format!("-----END {label}-----\n"));
    s
}

/// Write a freshly generated bring-up cert/key (see `test_kit::tls`'s module docs — never checked
/// in, generated fresh every test run) as PEM files in a unique temp dir; return their paths for
/// `rumqttd`'s `TlsConfig`.
fn write_bringup_pems(cert_der: &[u8], key_der: &[u8]) -> (String, String) {
    let dir = std::env::temp_dir().join(format!(
        "sentient-owned-tls-{}-{}",
        std::process::id(),
        reserve_ephemeral_port()
    ));
    std::fs::create_dir_all(&dir).expect("create temp cert dir");
    let cert_path = dir.join("bringup_cert.pem");
    let key_path = dir.join("bringup_key.pem");
    std::fs::write(&cert_path, to_pem("CERTIFICATE", cert_der)).expect("write cert pem");
    // The bring-up key is PKCS#8 DER — PEM label "PRIVATE KEY" (rumqttd reads it as Item::PKCS8Key).
    std::fs::write(&key_path, to_pem("PRIVATE KEY", key_der)).expect("write key pem");
    (
        cert_path.to_string_lossy().into_owned(),
        key_path.to_string_lossy().into_owned(),
    )
}

#[test]
fn owned_client_publishes_over_tls_to_real_rumqttd() {
    use lib_net::link::tls::{TlsStage, TLS_CHUNK};
    use rustls::pki_types::CertificateDer;

    // A fresh, self-signed identity (see `test_kit::tls`'s module docs) shared by both sides of
    // this test: the broker presents it (via the PEM files below), the owned client trusts its
    // cert as its sole root.
    let generated = test_kit::generate_bringup_identity();

    // The broker gets two listeners on one router: a TLS listener the owned client dials (presenting
    // the generated cert the client trusts) and a plaintext listener the rumqttc subscriber uses. A
    // PUBLISH arriving TLS-terminated on the first routes to the subscriber on the second.
    let tls_port = reserve_ephemeral_port();
    let plain_port = reserve_ephemeral_port();
    let (cert_path, key_path) = write_bringup_pems(&generated.cert_der, &generated.key_der);
    let _broker = start_broker(vec![
        Listener {
            name: "tls".to_string(),
            addr: SocketAddr::from(([127, 0, 0, 1], tls_port)),
            tls: Some(rumqttd::TlsConfig::Rustls {
                capath: None,
                certpath: cert_path,
                keypath: key_path,
            }),
        },
        Listener {
            name: "plain".to_string(),
            addr: SocketAddr::from(([127, 0, 0, 1], plain_port)),
            tls: None,
        },
    ]);

    let sub = spawn_subscriber(plain_port, "sentient/#");
    await_subscription(&sub);

    let (mut dev_client, mut dev_gateway) = LoopbackBridge::new(1514).split();
    let mut client_sockets: [SocketStorage; 1] = Default::default();
    let mut gateway_sockets: [SocketStorage; 1] = Default::default();
    let mut client_rx = [0u8; 4096];
    let mut client_tx = [0u8; 4096];
    let mut gateway_rx = [0u8; 4096];
    let mut gateway_tx = [0u8; 4096];

    let (mut client_stack, client_handle, mut gateway_stack, gateway_handle, mut tcp, mut now_ms) =
        establish(
            &mut dev_client,
            &mut dev_gateway,
            &mut client_sockets,
            &mut gateway_sockets,
            &mut client_rx,
            &mut client_tx,
            &mut gateway_rx,
            &mut gateway_tx,
            tls_port,
        );

    let opts = ConnectOptions {
        client_id: "sentient-owned-pub-tls",
        keep_alive_secs: 60,
        clean_session: true,
        username: None,
        password: None,
    };
    let mut session = MqttSession::new(opts);
    let mut tls = TlsStage::new_client(
        "broker.sentient-rs.local",
        CertificateDer::from(generated.cert_der),
    );

    let mut to_broker: VecDeque<u8> = VecDeque::new();
    let mut from_broker: VecDeque<u8> = VecDeque::new();
    let mut io_buf = [0u8; TLS_CHUNK];
    let mut plain = [0u8; 4096];
    let mut mqtt_out = [0u8; 4096];
    let mut published = false;
    let mut got: Option<(String, Vec<u8>)> = None;

    for _ in 0..MAX_POLLS {
        client_stack.poll(&mut dev_client, now_ms);
        gateway_stack.poll(&mut dev_gateway, now_ms);
        let mut progress = pump_gateway(
            &mut gateway_stack,
            gateway_handle,
            &mut tcp,
            &mut to_broker,
            &mut from_broker,
        );

        // Ciphertext from the socket into the TLS record layer.
        if client_stack.tcp(client_handle).can_recv() {
            let n = client_stack
                .tcp(client_handle)
                .recv_slice(&mut io_buf)
                .expect("client recv");
            if n > 0 {
                tls.feed_incoming(&io_buf[..n]).expect("client TLS feed");
            }
        }

        // Once the handshake is done, drive the MQTT session over decrypted plaintext.
        if !tls.is_handshaking() {
            let pn = tls.read_plaintext(&mut plain).expect("client TLS read");
            let out_n = session
                .poll(now_ms, &plain[..pn], &mut mqtt_out)
                .expect("session poll");
            if out_n > 0 {
                tls.write_plaintext(&mqtt_out[..out_n])
                    .expect("client TLS write");
                progress = true;
            }
            if session.is_connected() && !published {
                session.publish(TOPIC, PAYLOAD).expect("queue PUBLISH");
                published = true;
            }
        }

        // Drain any ciphertext (handshake flight or sealed record) back onto the socket.
        let cn = tls.drain_outgoing(&mut io_buf);
        if cn > 0 {
            client_stack
                .tcp(client_handle)
                .send_slice(&io_buf[..cn])
                .expect("client send");
            progress = true;
        }

        if let Ok(msg) = sub.msgs.try_recv() {
            got = Some(msg);
            break;
        }
        if !progress {
            std::thread::sleep(Duration::from_micros(500));
        }
        now_ms += 10;
    }

    assert!(
        !tls.is_handshaking(),
        "the owned client's TLS handshake with rumqttd should have completed"
    );
    let (topic, payload) =
        got.expect("subscriber should receive the owned client's TLS-delivered PUBLISH");
    assert_eq!(topic, TOPIC, "broker delivered the exact topic over TLS");
    assert_eq!(
        payload, PAYLOAD,
        "broker delivered the exact payload bytes over TLS"
    );
    assert!(
        published,
        "the owned client connected over TLS and published"
    );
}
