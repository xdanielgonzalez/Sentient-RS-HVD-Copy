//! Stock-client interoperability proof for `lib_net::link::mqtt_server::BrokerCore` (Phase-2 emulation
//! plan, Task T1): the whole point of hosting a broker on the device is that an **unmodified**
//! stock MQTT 3.1.1 client can connect to it and just work. This test pumps [`BrokerCore`] over a
//! real `std::net::TcpListener` on `127.0.0.1:0` — proving the caller-driven `feed_incoming`/
//! `drain_outgoing` seam actually carries real bytes over a real socket, not just an in-memory
//! loopback — and drives it with **`rumqttc`** (a genuine, off-the-shelf MQTT 3.1.1 client crate,
//! already a workspace dev-dependency for the `lib-mqtt`/`owned_broker_e2e` broker tests) acting as
//! an ordinary subscriber. Nothing about `BrokerCore` is special-cased for `rumqttc`; it never sees
//! which client library is on the other end.
//!
//! `BrokerCore` stores no payload bytes (see the "MEMORY-MODEL REDESIGN" — `mqtt_server`'s module
//! docs): the broker pump below renders each PUBLISH's payload on demand via a closure over a fixed
//! per-topic fixture table, exactly the seam the firmware's real renderer (canonical reading -> TMS
//! JSON) will occupy in a later step.
//!
//! Asserts the two retained-message code paths the Phase-2 architecture leans on for anti-flake
//! e2e (a late external subscriber must not race the firmware's publish cycle):
//! * **retained-on-subscribe**: `publish_topic(..., retain = true)` happens *before* the client
//!   subscribes; the client must still receive the rendered payload immediately as part of its
//!   SUBSCRIBE.
//! * **live publish**: a second `publish_topic` after the client is already subscribed must arrive
//!   too, rendered fresh.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use lib_net::link::mqtt_server::BrokerCore;
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};

const RETAINED_TOPIC: &str = "tms/METEOR-001/dc_measurement_update";
const RETAINED_PAYLOAD: &[u8] = b"{\"voltage\":54.52,\"amperage\":1.64}";
const LIVE_TOPIC: &str = "tms/METEOR-001/power_hardware_update";
const LIVE_PAYLOAD: &[u8] = b"{\"soc\":0.775,\"cycles\":2}";

/// One connected client's raw TCP socket, non-blocking so the pump thread can round-robin across
/// every client without one slow peer stalling the rest.
struct ClientConn {
    stream: TcpStream,
}

/// Runs `BrokerCore` against real sockets on a dedicated OS thread: accepts up to `MAX_CLIENTS`
/// connections on `listener`, and every tick reads available bytes into `feed_incoming`, drains
/// `drain_outgoing` back out (rendering payloads via [`render_fixture`]), and pumps any pending
/// `publish_topic` requests. Stops when `stop` is set.
struct BrokerThread {
    handle: JoinHandle<()>,
    stop: Arc<AtomicBool>,
}

/// A request the test drives into the broker thread: publish `topic` (topic-only — `BrokerCore`
/// stores no payload bytes; the broker pump's render closure supplies bytes from the fixture table
/// below at drain time), optionally retained. Sent over a channel so the test body (async, tokio)
/// and the broker pump (sync, blocking sockets) don't share `BrokerCore` across threads without
/// synchronization.
enum BrokerCmd {
    Publish { topic: &'static str, retain: bool },
}

/// The fixed per-topic fixture table the broker pump's render closure serves from — standing in for
/// the firmware's real renderer (canonical reading -> TMS JSON, a later step). `BrokerCore` never
/// sees this table directly; it only ever sees the topic string, and calls back into this closure
/// to get bytes at the moment a packet is actually framed.
const FIXTURES: &[(&str, &[u8])] = &[
    (RETAINED_TOPIC, RETAINED_PAYLOAD),
    (LIVE_TOPIC, LIVE_PAYLOAD),
];

/// The render closure itself: looks up `topic` in [`FIXTURES`] and copies its bytes into `buf`,
/// mirroring the shape `impl_app`'s real renderer will have (`FnMut(&str, &mut [u8]) -> Option<usize>`).
fn render_fixture(topic: &str, buf: &mut [u8]) -> Option<usize> {
    let (_, payload) = FIXTURES.iter().find(|(t, _)| *t == topic)?;
    if payload.len() > buf.len() {
        return None;
    }
    buf[..payload.len()].copy_from_slice(payload);
    Some(payload.len())
}

const MAX_CLIENTS: usize = 4;

fn spawn_broker_thread(
    listener: TcpListener,
) -> (BrokerThread, std::sync::mpsc::Sender<BrokerCmd>) {
    listener
        .set_nonblocking(true)
        .expect("nonblocking listener");
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop);
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<BrokerCmd>();

    let handle = std::thread::spawn(move || {
        let mut broker: BrokerCore<MAX_CLIENTS> = BrokerCore::new();
        let mut clients: [Option<ClientConn>; MAX_CLIENTS] = [None, None, None, None];

        while !stop_flag.load(Ordering::Relaxed) {
            // Accept a new connection into the first free slot (table full => drop it, `stream`
            // just goes out of scope).
            match listener.accept() {
                Ok((stream, _addr)) => {
                    stream
                        .set_nonblocking(true)
                        .expect("nonblocking client stream");
                    if let Some(idx) = clients.iter().position(|c| c.is_none()) {
                        broker.client_connected(idx);
                        if let Some(slot) = clients.get_mut(idx) {
                            *slot = Some(ClientConn { stream });
                        }
                    }
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(_) => {}
            }

            // Drain any publish commands from the test body.
            while let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    BrokerCmd::Publish { topic, retain } => broker.publish_topic(topic, retain),
                }
            }

            // Pump each connected client: read what's available, feed it in; drain outgoing and
            // write it out. A read/write error or clean EOF closes that slot.
            for (idx, slot) in clients.iter_mut().enumerate() {
                let Some(conn) = slot else { continue };
                let mut buf = [0u8; 1024];
                let mut closed = false;
                match conn.stream.read(&mut buf) {
                    Ok(0) => closed = true, // EOF
                    Ok(n) => {
                        if broker.feed_incoming(idx, &buf[..n]).is_err() {
                            closed = true;
                        }
                    }
                    Err(ref e) if e.kind() == ErrorKind::WouldBlock => {}
                    Err(_) => closed = true,
                }

                if !closed {
                    let mut out = [0u8; 8192];
                    loop {
                        let n = broker.drain_outgoing(idx, &mut out, &mut render_fixture);
                        if n == 0 {
                            break;
                        }
                        if conn.stream.write_all(&out[..n]).is_err() {
                            closed = true;
                            break;
                        }
                    }
                }

                if closed {
                    broker.client_closed(idx);
                    *slot = None;
                }
            }

            broker.poll(0);
            std::thread::sleep(Duration::from_millis(2));
        }
    });

    (BrokerThread { handle, stop }, cmd_tx)
}

impl BrokerThread {
    fn stop_and_join(self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.handle.join();
    }
}

/// Reserve a free ephemeral `127.0.0.1` port the same way the other broker e2e tests do (bind
/// `:0`, read it back, hand the *bound* listener onward rather than re-binding, so there's no
/// TOCTOU race with another process grabbing the port in between).
fn bind_ephemeral() -> TcpListener {
    TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port")
}

/// Wait (bounded) for the SUBACK, so publishes staged afterward aren't racing the subscription.
async fn wait_for_suback(eventloop: &mut rumqttc::EventLoop) {
    let deadline = Duration::from_secs(10);
    tokio::time::timeout(deadline, async {
        loop {
            if let Ok(Event::Incoming(Incoming::SubAck(_))) = eventloop.poll().await {
                return;
            }
        }
    })
    .await
    .expect("broker acknowledged the subscription");
}

/// Collect `Incoming::Publish` events until `want` have arrived or the bounded deadline elapses.
async fn collect_publishes(
    eventloop: &mut rumqttc::EventLoop,
    want: usize,
) -> Vec<rumqttc::Publish> {
    let deadline = Duration::from_secs(10);
    let mut got = Vec::new();
    let _ = tokio::time::timeout(deadline, async {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Incoming::Publish(p))) => {
                    got.push(p);
                    if got.len() >= want {
                        return;
                    }
                }
                Ok(_) => {}
                Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        }
    })
    .await;
    got
}

#[tokio::test]
async fn stock_rumqttc_client_receives_retained_and_live_publishes() {
    let listener = bind_ephemeral();
    let port = listener.local_addr().expect("local_addr").port();
    let (broker_thread, cmds) = spawn_broker_thread(listener);

    // The firmware publishes the retained TMS reading *before* any external subscriber connects —
    // this is the anti-flake path the Phase-2 e2e plan relies on (a late subscriber must not race
    // the poll cycle).
    cmds.send(BrokerCmd::Publish {
        topic: RETAINED_TOPIC,
        retain: true,
    })
    .expect("send retained publish command");

    // An ordinary, unmodified rumqttc client — nothing here is broker-specific.
    let mut opts = MqttOptions::new("stock-client", "127.0.0.1", port);
    opts.set_keep_alive(Duration::from_secs(5));
    let (client, mut eventloop) = AsyncClient::new(opts, 10);

    // First poll drives the CONNECT/CONNACK handshake.
    let connack_deadline = Duration::from_secs(10);
    tokio::time::timeout(connack_deadline, async {
        loop {
            if let Ok(Event::Incoming(Incoming::ConnAck(ack))) = eventloop.poll().await {
                assert_eq!(
                    ack.code,
                    rumqttc::ConnectReturnCode::Success,
                    "broker must accept a plain MQTT 3.1.1 CONNECT"
                );
                return;
            }
        }
    })
    .await
    .expect("CONNACK from the broker");

    client
        .subscribe("tms/#", QoS::AtMostOnce)
        .await
        .expect("subscribe");
    wait_for_suback(&mut eventloop).await;

    // Retained-on-subscribe: the message published before we even connected must arrive now.
    let retained = collect_publishes(&mut eventloop, 1).await;
    assert_eq!(
        retained.len(),
        1,
        "expected the retained message to replay on subscribe"
    );
    assert_eq!(retained[0].topic, RETAINED_TOPIC);
    assert_eq!(retained[0].payload.as_ref(), RETAINED_PAYLOAD);
    assert!(
        retained[0].retain,
        "replayed message must carry the RETAIN flag (MQTT 3.1.1 3.3.1.3)"
    );

    // Live publish: firmware publishes again, this time with a subscriber already attached.
    cmds.send(BrokerCmd::Publish {
        topic: LIVE_TOPIC,
        retain: true,
    })
    .expect("send live publish command");

    let live = collect_publishes(&mut eventloop, 1).await;
    assert_eq!(live.len(), 1, "expected the live publish to arrive");
    assert_eq!(live[0].topic, LIVE_TOPIC);
    assert_eq!(live[0].payload.as_ref(), LIVE_PAYLOAD);

    broker_thread.stop_and_join();
}
