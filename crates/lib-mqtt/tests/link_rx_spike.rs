//! SPIKE (gap-closure scaffolding, #237): does rumqttd 0.19's in-process `Broker::link` `LinkRx`
//! actually deliver publishes in-process, and does `LinkTx` expose any retain capability?
//!
//! `impl_local_broker.rs:78` currently does `let (link_tx, _link_rx) = broker.link(...)` and
//! drops `_link_rx` entirely — nothing ever subscribes through the Link or drains it. The TMS
//! Request/Reply design (`bin-rpi/src/tasks/reply_listener.rs`) needs to know whether the Link
//! can be used bidirectionally (subscribe + publish + receive, all in-process, no loopback TCP
//! client) before it's designed around that assumption.
//!
//! **IMPORTANT — environment caveat:** this test was authored in a sandboxed session with no
//! network access to the crate registry (`rust-mirror`/crates.io are both unreachable — see the
//! session's structured output), so it could **not** be compiled or run here. The method calls
//! below (`LinkTx::subscribe`, `LinkRx::recv`) are a best-effort reconstruction of the rumqttd
//! 0.19 API from the already-*confirmed-compiling* call sites in `impl_local_broker.rs`
//! (`broker.link(id) -> (LinkTx, LinkRx)`, and the `rumqttd::Config`/`ServerSettings`/
//! `RouterConfig`/`ConnectionSettings` construction mirrored below verbatim) and
//! `impl_broker_publisher.rs` (`link.publish(topic: String, payload: Vec<u8>) -> Result<_, _>` —
//! note: exactly two arguments, no retain flag, which is itself the answer to the retain half of
//! this spike; see the structured output's `spike_retain`). **A follow-up agent with real
//! registry access must run `cargo test -p lib-mqtt --test link_rx_spike -- --ignored` to confirm
//! this actually compiles and passes before the Request/Reply design leans on it.**
//!
//! Fallback if `LinkRx` does not deliver (or this doesn't compile as written): a loopback
//! `rumqttc` TCP client — the pattern already used and proven working by `test-kit::start_broker`
//! + `test-e2e`'s rumqttc-based tests (`crates/test-e2e/tests/mqtt_e2e.rs` et al.).

// Test code may unwrap/panic; the workspace denies these in logic only.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::mpsc;
use std::time::Duration;

/// Boot a minimal rumqttd broker exactly like `LocalBroker::start` (`impl_local_broker.rs`) does,
/// but keep BOTH halves of the in-process link (that call site drops `_link_rx`) so this spike can
/// probe it. Loopback-only, OS-assigned ephemeral port — never dialed into over TCP by this test.
fn start_broker_keeping_both_link_halves() -> (rumqttd::local::LinkTx, rumqttd::local::LinkRx) {
    let listen_addr: SocketAddr = "127.0.0.1:0".parse().expect("valid loopback addr");

    let connection_settings = rumqttd::ConnectionSettings {
        connection_timeout_ms: 10000,
        max_payload_size: 1024 * 1024,
        max_inflight_count: 100,
        auth: None,
        external_auth: None,
        dynamic_filters: false,
    };

    let mut v4_map = HashMap::new();
    v4_map.insert(
        "spike".to_string(),
        rumqttd::ServerSettings {
            name: "spike".to_string(),
            listen: listen_addr,
            tls: None,
            next_connection_delay_ms: 0,
            connections: connection_settings,
        },
    );

    let router_config = rumqttd::RouterConfig {
        max_connections: 10,
        max_outgoing_packet_count: 100,
        max_segment_size: 65536,
        max_segment_count: 10,
        custom_segment: None,
        initialized_filters: None,
        shared_subscriptions_strategy: Default::default(),
    };

    let broker_config = rumqttd::Config {
        id: 0,
        router: router_config,
        v4: Some(v4_map),
        v5: None,
        ws: None,
        cluster: None,
        console: None,
        bridge: None,
        prometheus: None,
        metrics: None,
    };

    let mut broker = rumqttd::Broker::new(broker_config);
    let (link_tx, link_rx) = broker.link("link-rx-spike").expect("in-process link");
    std::thread::spawn(move || {
        let _ = broker.start();
    });
    (link_tx, link_rx)
}

/// Subscribe and publish over the SAME in-process Link, then check whether the publish is
/// delivered back to `LinkRx`. `#[ignore]` by default: `LinkRx::recv` is (per recollection, see
/// the module doc comment's caveat) blocking, so this must not run unattended in a CI lane that
/// doesn't know to bound it — the timeout below is a best-effort bound via a dedicated thread,
/// not a guarantee.
#[test]
#[ignore = "SPIKE: unverified in the authoring sandbox (no registry access) — run explicitly with --ignored once rumqttd is fetchable"]
fn link_rx_delivers_publishes_made_on_the_same_link() {
    let (mut link_tx, mut link_rx) = start_broker_keeping_both_link_halves();
    // Give the router a moment to come up before subscribing.
    std::thread::sleep(Duration::from_millis(200));

    // Best-effort: `LinkTx::subscribe` is not exercised anywhere else in this workspace's
    // currently-compiling code, so this signature is unverified.
    link_tx
        .subscribe("spike/#")
        .expect("subscribe over the in-process link");

    link_tx
        .publish("spike/topic".to_string(), b"spike-payload".to_vec())
        .expect("publish over the in-process link");

    // Bound `LinkRx::recv` (documented as blocking) with a dedicated thread + timeout, since a
    // negative result (no delivery) must not hang the test suite forever.
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = link_rx.recv();
        let _ = tx.send(format!("{result:?}"));
    });

    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(notification_debug) => {
            println!("link_rx_spike: LinkRx.recv() returned: {notification_debug}");
            // The precise `Notification` shape is unverified here; a real run should assert it
            // contains the published topic/payload (a `Forward`/`Publish`-shaped variant).
        }
        Err(_) => panic!(
            "link_rx_spike: LinkRx.recv() did not deliver anything within 5s — \
             LinkRx does NOT appear to receive same-Link publishes this way (or subscribe/publish \
             failed silently); see this test's module doc comment for the loopback rumqttc fallback"
        ),
    }
}
