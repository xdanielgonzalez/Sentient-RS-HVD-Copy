//! Broker-side CONNECT authentication (`lib_mqtt::broker::seam_auth`) against the REAL
//! `lib-security` software `TokenVerifier` — the concrete backend PR #93's seam was wired
//! ahead of. `test_kit::FakeTokenVerifier`'s own doc comment flags the exact gap this closes:
//! "`lib-security` ships no concrete `TokenVerifier` implementation anywhere in this workspace
//! yet ... A production deployment MUST bind a real one before this path is trustworthy."
//!
//! Tokens here are minted with `lib_security::backends::software::sign_token` (the paired
//! signer for `SoftwareTokenVerifier`) against a real `SoftwareKeyStore`-held ECDSA P-256 key —
//! never a shared-secret string compare — and exercised over the real broker + a real network
//! `rumqttc` client, mirroring `broker_auth.rs`'s `FakeTokenVerifier` coverage (accept a valid,
//! correctly-scoped token; reject a tampered one) but through the production cryptographic path.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};

use lib_mqtt::broker::{seam_auth, BrokerConfig, LocalBroker};
use lib_security::backends::software::{
    sign_token, InsecureTestRng, SoftwareKeyStore, SoftwareTokenVerifier, PUBLIC_KEY_LEN, TOKEN_MAX,
};
use lib_security::{Algorithm, KeyStore};

use test_kit::reserve_ephemeral_port;

const ISSUER: &str = "sentient-test-issuer";
const AUDIENCE: &str = "lib-mqtt-broker";

/// A real issuer: a software `KeyStore` holding one ECDSA P-256 key, and that key's exported
/// public half (what a deployment would hand to the broker as `trusted_key`).
fn issuer() -> (
    SoftwareKeyStore<InsecureTestRng>,
    lib_security::KeyHandle,
    Vec<u8>,
) {
    let mut store = SoftwareKeyStore::new(InsecureTestRng::new(0x00C0_FFEE));
    let handle = store
        .generate(Algorithm::EcdsaP256Sha256)
        .expect("software KeyStore generates a real ECDSA P-256 key");
    let mut public_key = [0u8; PUBLIC_KEY_LEN];
    let pk_len = store
        .export_public(handle, &mut public_key)
        .expect("export_public");
    (store, handle, public_key[..pk_len].to_vec())
}

/// Mint a REAL, validly-signed token for `subject`, far from expiry — as a `String`: the whole
/// token is guaranteed ASCII (claims text + hex-encoded signature, see
/// `lib_security::backends::software::token`'s module doc), which is load-bearing here since
/// the MQTT CONNECT password rumqttc/rumqttd carry it over is typed `String` end-to-end.
fn valid_token(
    store: &SoftwareKeyStore<InsecureTestRng>,
    handle: lib_security::KeyHandle,
    subject: &str,
) -> String {
    let mut token = [0u8; TOKEN_MAX];
    let len = sign_token(
        store,
        handle,
        subject,
        ISSUER,
        AUDIENCE,
        9_999_999_999,
        &mut token,
    )
    .expect("sign_token mints a real, verifiable token");
    String::from_utf8(token[..len].to_vec()).expect("the software token wire format is ASCII")
}

/// Start the embedded broker, on a freshly reserved ephemeral port, with CONNECT authorization
/// enabled via `seam_auth` over the REAL `SoftwareTokenVerifier` (not the `FakeTokenVerifier` test
/// double `support::start_authenticated_broker` uses).
fn start_broker_with_real_verifier(trusted_key: Vec<u8>) -> (LocalBroker, u16) {
    for _ in 0..8 {
        let port = reserve_ephemeral_port();
        let cfg = BrokerConfig {
            id: "sentient".into(),
            host: "127.0.0.1".into(),
            port,
            auth: Some(seam_auth(
                Arc::new(SoftwareTokenVerifier),
                trusted_key.clone(),
                ISSUER.into(),
                AUDIENCE.into(),
            )),
        };
        if let Ok(b) = LocalBroker::start(cfg) {
            return (b, port);
        }
    }
    panic!("could not bind a test broker on any reserved ephemeral port after 8 attempts");
}

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
async fn real_verifier_accepts_a_genuinely_signed_token_and_publishes() {
    let (store, handle, trusted_key) = issuer();
    let (broker, port) = start_broker_with_real_verifier(trusted_key);

    let mut sub_opts = MqttOptions::new("sentient-real-auth-sub", "127.0.0.1", port);
    sub_opts.set_keep_alive(Duration::from_secs(5));
    sub_opts.set_credentials("device-1", valid_token(&store, handle, "device-1"));
    let (sub_client, mut sub_eventloop) = AsyncClient::new(sub_opts, 10);
    sub_client
        .subscribe("tms/#", QoS::AtLeastOnce)
        .await
        .expect("subscribe");
    let acked = pump_until(&mut sub_eventloop, |pkt| matches!(pkt, Incoming::SubAck(_))).await;
    assert!(
        acked.is_some(),
        "a genuinely ECDSA-signed token's CONNECT was accepted by the REAL SoftwareTokenVerifier"
    );

    let mut pub_opts = MqttOptions::new("sentient-real-auth-pub", "127.0.0.1", port);
    pub_opts.set_keep_alive(Duration::from_secs(5));
    pub_opts.set_credentials("device-2", valid_token(&store, handle, "device-2"));
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
            "tms/METEOR-001/real_auth_ok",
            QoS::AtLeastOnce,
            false,
            b"{\"ok\":true}".to_vec(),
        )
        .await
        .expect("publish accepted by a client the real verifier authenticated");

    let got = pump_until(&mut sub_eventloop, |pkt| {
        matches!(pkt, Incoming::Publish(_))
    })
    .await;
    match got {
        Some(Incoming::Publish(p)) => {
            assert_eq!(p.topic, "tms/METEOR-001/real_auth_ok");
            assert_eq!(p.payload.as_ref(), b"{\"ok\":true}");
        }
        _ => panic!("authenticated publish must reach the subscriber"),
    }

    pump_task.abort();
    assert!(broker.is_running());
}

#[tokio::test]
async fn real_verifier_rejects_a_tampered_signature() {
    let (store, handle, trusted_key) = issuer();
    let (_broker, port) = start_broker_with_real_verifier(trusted_key);

    // A well-formed token whose trailing hex digit (inside the signature) has been flipped: the
    // ECDSA signature no longer verifies, so `SoftwareTokenVerifier::verify_token` denies with
    // `VerificationFailed`, which `seam_auth` maps to a flat CONNECT refusal — no CONNACK, ever.
    // Flipping a hex DIGIT (not an arbitrary byte XOR) keeps the tampered token valid ASCII, which
    // is load-bearing for `set_credentials`'s `String` parameter.
    let mut tampered = valid_token(&store, handle, "device-evil").into_bytes();
    let last = tampered.len() - 1;
    tampered[last] = if tampered[last] == b'0' { b'1' } else { b'0' };
    let tampered = String::from_utf8(tampered).expect("a hex-digit flip keeps the token ASCII");

    let mut bad_opts = MqttOptions::new("sentient-real-auth-bad", "127.0.0.1", port);
    bad_opts.set_keep_alive(Duration::from_secs(5));
    bad_opts.set_credentials("device-evil", tampered);
    let (bad_client, mut bad_eventloop) = AsyncClient::new(bad_opts, 10);

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
        "a token with a tampered signature must never be accepted by the real verifier"
    );
}
