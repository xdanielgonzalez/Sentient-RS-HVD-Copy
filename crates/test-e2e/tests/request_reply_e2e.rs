//! Request/Reply control-path e2e (#246) — a real embedded broker, an `EnergyStartStopRequest`
//! published on `tms/{device}/energy_start_stop_request`, and the standard's 0.1 s deadline: a
//! `Reply` on `tms/{device}/reply` must arrive before the budget elapses, echoing the request's
//! `requestingDeviceId` + `sequenceId` and reporting `REPLY_NOT_IMPLEMENTED` (this adapter proxies
//! a STOR device with no command surface — see `docs/tms-compliance.md`).
//!
//! `bin-rpi::tasks::reply_listener::run_reply_listener` is the real production implementation of
//! this responder — but `bin-rpi` is a `[[bin]]`-only crate (no library target), so it cannot be
//! imported here; this test instead runs a local `respond_to_requests` task that mirrors
//! `reply_listener`'s exact wire-level shape (subscribe the named request topic over a loopback
//! `rumqttc` client -> decode -> build `Reply` -> publish `tms/{device}/reply`), proving the
//! mechanism and the latency budget are achievable against the real rumqttd broker `bin-rpi`
//! embeds. See `lib_mqtt::broker::impl_broker_subscriber`'s doc comment for why the responder
//! connects over loopback TCP rather than the in-process `LinkRx`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use heapless::String as HString;
use lib_mapping::schema::{
    topics, ConfigId, DeviceConfigRequest, EnergyStartStopLevel, EnergyStartStopRequest, Identity,
    Reply, ReplyCode, ReplyStatus, String32, INVALID_PORT_NUMBER,
};
use lib_mapping::sink::{topic_into, MAX_PAYLOAD, MAX_TOPIC};
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};

use test_kit::start_broker;

/// TMS's required Request -> Reply deadline (MIL-STD-3071; see `docs/gap-analysis-meteor-parity.md`'s
/// "Request/Reply control path" finding).
const TMS_BUDGET: Duration = Duration::from_millis(100);

fn dev_id() -> HString<32> {
    let mut s = HString::new();
    s.push_str("METEOR-001")
        .expect("METEOR-001 fits String<32>");
    s
}

fn hid(s: &str) -> Identity {
    let mut h = Identity::new();
    h.push_str(s).expect("fits Identity");
    h
}

fn reason(s: &str) -> String32 {
    let mut h = String32::new();
    h.push_str(s).expect("fits String32");
    h
}

fn topic(tms_topic: &str) -> HString<MAX_TOPIC> {
    let mut out = HString::new();
    topic_into("tms", "METEOR-001", tms_topic, &mut out).expect("topic fits the buffer");
    out
}

/// Mirrors `bin_rpi::tasks::reply_listener::run_reply_listener`'s core loop (see this file's top
/// doc comment for why it is reimplemented here rather than imported). A request whose payload
/// carries a `request_id.requesting_device_id` + `sequence_id` gets `ReplyCode::NotImplemented`
/// echoing them; an undecodable payload still gets a `Reply` (`ReplyCode::BadRequest`) within
/// budget rather than being silently dropped.
async fn respond_to_requests(port: u16, mut shutdown: tokio::sync::oneshot::Receiver<()>) {
    let mut opts = MqttOptions::new("test-e2e-responder", "127.0.0.1", port);
    opts.set_keep_alive(Duration::from_secs(5));
    let (client, mut eventloop) = AsyncClient::new(opts, 10);
    let request_topic = topic(topics::ENERGY_START_STOP_REQUEST);
    client
        .subscribe(request_topic.as_str(), QoS::AtLeastOnce)
        .await
        .expect("responder subscribes to the request topic");

    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            event = eventloop.poll() => {
                match event {
                    Ok(Event::Incoming(Incoming::Publish(p))) if p.topic == request_topic.as_str() => {
                        // Untyped JSON decode (this workspace's e2e convention, avoids a
                        // serde-json-core dev-dep here). A non-JSON / field-less payload must still
                        // get a Reply — never panic this task — hence `.ok()`, not `.expect(..)`.
                        let parsed: Option<serde_json::Value> =
                            serde_json::from_slice(&p.payload).ok();
                        let requesting = parsed
                            .as_ref()
                            .and_then(|v| v.get("request_id"))
                            .and_then(|r| r.get("requesting_device_id"))
                            .and_then(|s| s.as_str());
                        let sequence = parsed
                            .as_ref()
                            .and_then(|v| v.get("sequence_id"))
                            .and_then(|s| s.as_u64());
                        let reply = match (requesting, sequence) {
                            (Some(id), Some(seq)) => Reply {
                                requesting_device_id: hid(id),
                                target_device_id: dev_id(),
                                config: ConfigId::Unknown,
                                port_number: INVALID_PORT_NUMBER,
                                request_sequence_id: seq,
                                status: ReplyStatus {
                                    code: ReplyCode::NotImplemented,
                                    reason: reason("not implemented"),
                                },
                            },
                            _ => Reply {
                                requesting_device_id: Identity::new(),
                                target_device_id: dev_id(),
                                config: ConfigId::Unknown,
                                port_number: INVALID_PORT_NUMBER,
                                request_sequence_id: 0,
                                status: ReplyStatus {
                                    code: ReplyCode::BadRequest,
                                    reason: reason("malformed request payload"),
                                },
                            },
                        };
                        let mut buf = [0u8; MAX_PAYLOAD];
                        let n = lib_mapping::codec::to_json_slice(&reply, &mut buf)
                            .expect("encode reply");
                        let reply_topic = topic(topics::REPLY);
                        client
                            .try_publish(reply_topic.as_str(), QoS::AtLeastOnce, false, &buf[..n])
                            .expect("publish reply");
                    }
                    Ok(_) => {}
                    Err(_) => tokio::time::sleep(Duration::from_millis(5)).await,
                }
            }
        }
    }
}

/// Wait for the subscription SubAck before publishing (no retained messages here) — mirrors
/// `broker_roundtrip.rs`/`multi_port_e2e.rs`'s identical helper.
async fn wait_for_suback(eventloop: &mut rumqttc::EventLoop) {
    let deadline = Duration::from_secs(5);
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

/// A well-formed `EnergyStartStopRequest` from MC-1 with sequence 42.
fn sample_request() -> EnergyStartStopRequest {
    EnergyStartStopRequest {
        request_id: DeviceConfigRequest {
            requesting_device_id: hid("MC-1"),
            target_device_id: dev_id(),
            config: ConfigId::Active,
        },
        sequence_id: 42,
        from_level: EnergyStartStopLevel::Any,
        to_level: EnergyStartStopLevel::Operational,
        switch_conditions: None,
    }
}

#[tokio::test]
async fn request_reply_round_trips_under_the_tms_budget() {
    let (broker, port) = start_broker();
    let _keep_broker_alive = broker;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let responder = tokio::spawn(respond_to_requests(port, shutdown_rx));

    let mut opts = MqttOptions::new("test-e2e-requester", "127.0.0.1", port);
    opts.set_keep_alive(Duration::from_secs(5));
    let (client, mut eventloop) = AsyncClient::new(opts, 10);
    let reply_topic = topic(topics::REPLY);
    client
        .subscribe(reply_topic.as_str(), QoS::AtLeastOnce)
        .await
        .expect("requester subscribes to the reply topic");
    wait_for_suback(&mut eventloop).await;

    // Give the responder a moment to finish its own subscribe before the request is published.
    tokio::time::sleep(Duration::from_millis(150)).await;

    let req = sample_request();
    let mut buf = [0u8; MAX_PAYLOAD];
    let n = lib_mapping::codec::to_json_slice(&req, &mut buf).expect("encode request");
    let request_topic = topic(topics::ENERGY_START_STOP_REQUEST);

    let started = Instant::now();
    client
        .publish(request_topic.as_str(), QoS::AtLeastOnce, false, &buf[..n])
        .await
        .expect("publish request");

    let deadline = Duration::from_secs(5);
    let got = tokio::time::timeout(deadline, async {
        loop {
            if let Ok(Event::Incoming(Incoming::Publish(p))) = eventloop.poll().await {
                if p.topic == reply_topic.as_str() {
                    return p;
                }
            }
        }
    })
    .await
    .expect("a Reply arrived within the overall test deadline");
    let elapsed = started.elapsed();

    let v: serde_json::Value =
        serde_json::from_slice(&got.payload).expect("reply payload is valid json");
    assert_eq!(v["requesting_device_id"], "MC-1");
    assert_eq!(v["request_sequence_id"], 42);
    assert_eq!(v["status"]["code"], "REPLY_NOT_IMPLEMENTED");
    assert!(
        elapsed < TMS_BUDGET,
        "request-to-reply round trip took {elapsed:?}, exceeding the TMS {TMS_BUDGET:?} budget"
    );

    let _ = shutdown_tx.send(());
    responder.await.ok();
}

/// A malformed (non-JSON) request must still get a `Reply` (`REPLY_BAD_REQUEST`) within budget, not
/// silence — matching the standard's "a Reply within 0.1s of any request" requirement regardless
/// of the request's own validity.
#[tokio::test]
async fn malformed_request_still_gets_a_reply_within_budget() {
    let (broker, port) = start_broker();
    let _keep_broker_alive = broker;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let responder = tokio::spawn(respond_to_requests(port, shutdown_rx));

    let mut opts = MqttOptions::new("test-e2e-requester-bad", "127.0.0.1", port);
    opts.set_keep_alive(Duration::from_secs(5));
    let (client, mut eventloop) = AsyncClient::new(opts, 10);
    let reply_topic = topic(topics::REPLY);
    client
        .subscribe(reply_topic.as_str(), QoS::AtLeastOnce)
        .await
        .expect("requester subscribes to the reply topic");
    wait_for_suback(&mut eventloop).await;
    tokio::time::sleep(Duration::from_millis(150)).await;

    let request_topic = topic(topics::ENERGY_START_STOP_REQUEST);
    let started = Instant::now();
    client
        .publish(
            request_topic.as_str(),
            QoS::AtLeastOnce,
            false,
            b"not json at all".to_vec(),
        )
        .await
        .expect("publish malformed request");

    let deadline = Duration::from_secs(5);
    let got = tokio::time::timeout(deadline, async {
        loop {
            if let Ok(Event::Incoming(Incoming::Publish(p))) = eventloop.poll().await {
                if p.topic == reply_topic.as_str() {
                    return p;
                }
            }
        }
    })
    .await
    .expect("a Reply arrived within the overall test deadline");
    let elapsed = started.elapsed();

    let v: serde_json::Value =
        serde_json::from_slice(&got.payload).expect("reply payload is valid json");
    assert_eq!(v["status"]["code"], "REPLY_BAD_REQUEST");
    assert!(
        elapsed < TMS_BUDGET,
        "malformed-request round trip took {elapsed:?}, exceeding the TMS {TMS_BUDGET:?} budget"
    );

    let _ = shutdown_tx.send(());
    responder.await.ok();
}
