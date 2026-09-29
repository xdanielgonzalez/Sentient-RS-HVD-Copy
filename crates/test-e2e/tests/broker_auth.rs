//! Broker-side MQTT CONNECT authentication, enforced through the `lib-security` seam.
//!
//! `LocalBroker` plugs an rumqttd `external_auth` hook (see `lib_mqtt::broker::seam_auth`)
//! that verifies the CONNECT password as a token via `lib_security::TokenVerifier`, fail
//! closed. These tests exercise both directions end-to-end over the real broker + a real network
//! `rumqttc` client (not the in-process Link):
//!
//! * a client presenting a valid, correctly-scoped token CONNECTs and can publish;
//! * a client presenting a bad-credential token is refused at CONNECT — it never receives a
//!   CONNACK, and nothing it attempted to publish ever reaches a subscriber.
//!
//! The credential backend here (`test_kit::FakeTokenVerifier`) is a test double standing in for a
//! bound `lib-security` token backend (see its doc comment for the flagged gap); the wiring
//! under test — rumqttd's `external_auth` hook calling through `TokenVerifier::verify_token` — is
//! the real production path.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};

use test_kit::{invalid_token, start_authenticated_broker, valid_token};

/// Drive `eventloop` until an incoming packet matches `pred`, or the deadline elapses.
async fn pump_until<F>(eventloop: &mut rumqttc::EventLoop, mut pred: F) -> Option<Incoming>
where
    F: FnMut(&Incoming) -> bool,
{
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(pkt)) => {
                    if pred(&pkt) {
                        return pkt;
                    }
                }
                Ok(Event::Outgoing(_)) => {}
                Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        }
    })
    .await
    .ok()
}

#[tokio::test]
async fn authenticated_client_connects_and_publishes() {
    let (broker, port) = start_authenticated_broker();

    // A valid-token subscriber, to observe the publish arriving.
    let mut sub_opts = MqttOptions::new("sentient-auth-sub", "127.0.0.1", port);
    sub_opts.set_keep_alive(Duration::from_secs(5));
    sub_opts.set_credentials("device-1", valid_token("device-1"));
    let (sub_client, mut sub_eventloop) = AsyncClient::new(sub_opts, 10);
    sub_client
        .subscribe("tms/#", QoS::AtLeastOnce)
        .await
        .expect("subscribe");
    let acked = pump_until(&mut sub_eventloop, |pkt| matches!(pkt, Incoming::SubAck(_))).await;
    assert!(
        acked.is_some(),
        "valid-token subscriber's CONNECT was accepted and its subscription acked"
    );

    // A valid-token publisher, over a real network connection (not the in-process Link).
    let mut pub_opts = MqttOptions::new("sentient-auth-pub", "127.0.0.1", port);
    pub_opts.set_keep_alive(Duration::from_secs(5));
    pub_opts.set_credentials("device-2", valid_token("device-2"));
    let (pub_client, mut pub_eventloop) = AsyncClient::new(pub_opts, 10);
    let pump_task = tokio::spawn(async move {
        loop {
            if pub_eventloop.poll().await.is_err() {
                break;
            }
        }
    });

    pub_client
        .publish(
            "tms/METEOR-001/auth_ok",
            QoS::AtLeastOnce,
            false,
            b"{\"ok\":true}".to_vec(),
        )
        .await
        .expect("publish accepted by an authenticated client");

    let got = pump_until(&mut sub_eventloop, |pkt| {
        matches!(pkt, Incoming::Publish(_))
    })
    .await;
    match got {
        Some(Incoming::Publish(p)) => {
            assert_eq!(p.topic, "tms/METEOR-001/auth_ok");
            assert_eq!(p.payload.as_ref(), b"{\"ok\":true}");
        }
        _ => panic!("authenticated publish must reach the subscriber"),
    }

    pump_task.abort();
    assert!(broker.is_running());
}

#[tokio::test]
async fn bad_credential_client_is_rejected() {
    let (_broker, port) = start_authenticated_broker();

    // A valid-token subscriber, present the whole time — it must NEVER see the bad client's
    // publish, whether or not the bad client's own CONNECT is observed to fail.
    let mut sub_opts = MqttOptions::new("sentient-auth-sub-2", "127.0.0.1", port);
    sub_opts.set_keep_alive(Duration::from_secs(5));
    sub_opts.set_credentials("device-1", valid_token("device-1"));
    let (sub_client, mut sub_eventloop) = AsyncClient::new(sub_opts, 10);
    sub_client
        .subscribe("tms/#", QoS::AtLeastOnce)
        .await
        .expect("subscribe");
    let acked = pump_until(&mut sub_eventloop, |pkt| matches!(pkt, Incoming::SubAck(_))).await;
    assert!(acked.is_some());

    // A bad-credential client: a well-formed token, wrong shared secret. `seam_auth` denies it via
    // `TokenVerifier::verify_token` returning `Err(VerificationFailed)` -> the broker never sends a
    // CONNACK and drops the connection (rumqttd's `external_auth` contract: reject = refuse, not
    // "connect then kick").
    let mut bad_opts = MqttOptions::new("sentient-auth-bad", "127.0.0.1", port);
    bad_opts.set_keep_alive(Duration::from_secs(5));
    bad_opts.set_credentials("device-evil", invalid_token("device-evil"));
    let (bad_client, mut bad_eventloop) = AsyncClient::new(bad_opts, 10);

    // Queue a publish attempt (rumqttc just enqueues; nothing goes over the wire without a
    // successful CONNECT, which this client must never get).
    let _ = bad_client
        .publish(
            "tms/METEOR-001/should_not_arrive",
            QoS::AtLeastOnce,
            false,
            b"{\"ok\":false}".to_vec(),
        )
        .await;

    let bad_saw_connack = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match bad_eventloop.poll().await {
                Ok(Event::Incoming(Incoming::ConnAck(_))) => return true,
                Ok(_) => continue,
                Err(_) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);
    assert!(
        !bad_saw_connack,
        "a bad-credential client must never receive a ConnAck"
    );

    // The authenticated subscriber must never observe the rejected client's publish.
    let leaked = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match sub_eventloop.poll().await {
                Ok(Event::Incoming(Incoming::Publish(p))) => return Some(p),
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
    })
    .await
    .ok()
    .flatten();
    assert!(
        leaked.is_none(),
        "an unauthorized client's publish must never reach the topic"
    );
}
