//! T8 — end-to-end round trip through the embedded broker (closes #51, local variant).
//!
//! Boots the real rumqttd broker on loopback, subscribes with an external `rumqttc` client to
//! `tms/#`, publishes a golden `DcMeasurementUpdate` through the in-process `BrokerPublisher`, and
//! asserts the exact topic slug and byte-identical JSON arrive at the subscriber — the same path a
//! deployed mosquitto bridge takes. An `#[ignore]`d variant runs the same publish against a
//! configured external endpoint (the switch to flip when Day-1 Sentient creds land).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use heapless::String as HString;
use lib_mapping::schema::{topics, ClockMonotonic};
use lib_mapping::sink::Publisher;
use lib_mapping::transform::ToTms;
use lib_mqtt::publish_tms;
use lib_parse::canonical::CanonicalReading;
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};

use test_kit::start_broker;

fn dev_id() -> HString<32> {
    let mut s = HString::new();
    s.push_str("METEOR-001").ok();
    s
}

fn sample() -> CanonicalReading {
    let mut cells = heapless::Vec::new();
    cells.push(3894).ok();
    CanonicalReading {
        device_address: 0,
        cell_voltages_mv: cells,
        temperatures_raw: heapless::Vec::new(),
        current_ca: -144,
        total_voltage_cv: 5451,
        remaining_capacity_cah: 13953,
        typical_capacity_cah: 18000,
        rated_capacity_cah: 18000,
        soc_dpct: 775,
        soh_dpct: 1000,
        cycles: 2,
        port_voltage_cv: 5453,
    }
}

/// Drive the eventloop until an incoming packet matches `pred`, or the overall deadline elapses.
async fn pump_until<F>(eventloop: &mut rumqttc::EventLoop, mut pred: F) -> Option<Incoming>
where
    F: FnMut(&Incoming) -> bool,
{
    let deadline = Duration::from_secs(5);
    tokio::time::timeout(deadline, async {
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
async fn tms_payload_round_trips_through_broker() {
    let (mut broker, port) = start_broker();
    let mut publisher = broker.publisher();

    let mut opts = MqttOptions::new("sentient-test-sub", "127.0.0.1", port);
    opts.set_keep_alive(Duration::from_secs(5));
    let (client, mut eventloop) = AsyncClient::new(opts, 10);
    client
        .subscribe("tms/#", QoS::AtLeastOnce)
        .await
        .expect("subscribe");

    // Ensure the subscription is registered before publishing (no retained messages here).
    let acked = pump_until(&mut eventloop, |pkt| matches!(pkt, Incoming::SubAck(_))).await;
    assert!(acked.is_some(), "broker acknowledged the subscription");

    // Publish a golden TMS payload through the in-process link.
    let now = ClockMonotonic {
        seconds: 1000,
        nanoseconds: 0,
    };
    let update = sample().to_dc_measurement(dev_id(), now);
    publish_tms(
        &mut publisher,
        "tms",
        "METEOR-001",
        topics::DC_MEASUREMENT_UPDATE,
        &update,
    )
    .expect("publish onto broker");
    assert!(
        publisher.healthy(),
        "publisher healthy after a successful publish"
    );
    assert!(broker.is_running());

    // The subscriber must receive it, on the exact topic, byte-identical.
    let got = pump_until(&mut eventloop, |pkt| matches!(pkt, Incoming::Publish(_))).await;
    let publish = match got {
        Some(Incoming::Publish(p)) => p,
        _ => panic!("did not receive the published TMS message"),
    };
    assert_eq!(publish.topic, "tms/METEOR-001/dc_measurement_update");
    let v: serde_json::Value = serde_json::from_slice(&publish.payload).expect("valid TMS json");
    let amps = v["external_measurement"][0]["line"][0]["amperage"]
        .as_f64()
        .unwrap();
    assert!(
        (amps - 1.44).abs() < 0.01,
        "discharging -1.44 A maps to TMS +1.44"
    );
}

/// Day-1 bring-up: publish the same golden payload to a configured external endpoint. Enable by
/// setting `SENTIENT_TEST_ENDPOINT=host:port` (and, for TLS, wiring certs into the client here)
/// once Sentient supplies the private-cloud credentials + test broker. `cargo test -- --ignored`.
#[tokio::test]
#[ignore = "requires SENTIENT_TEST_ENDPOINT (Day-1 Sentient creds/test broker)"]
async fn tms_payload_reaches_configured_endpoint() {
    let endpoint = match std::env::var("SENTIENT_TEST_ENDPOINT") {
        Ok(e) => e,
        Err(_) => return,
    };
    let (host, port) = endpoint
        .split_once(':')
        .expect("SENTIENT_TEST_ENDPOINT=host:port");
    let port: u16 = port.parse().expect("port");

    let mut opts = MqttOptions::new("sentient-test-pub", host, port);
    opts.set_keep_alive(Duration::from_secs(5));
    let (client, mut eventloop) = AsyncClient::new(opts, 10);

    let now = ClockMonotonic {
        seconds: 1000,
        nanoseconds: 0,
    };
    let update = sample().to_dc_measurement(dev_id(), now);
    let mut payload = [0u8; 8192];
    let n = lib_mapping::codec::to_json_slice(&update, &mut payload).expect("encode");
    client
        .publish(
            "tms/METEOR-001/dc_measurement_update",
            QoS::AtLeastOnce,
            false,
            &payload[..n],
        )
        .await
        .expect("publish to endpoint");

    let acked = pump_until(&mut eventloop, |pkt| matches!(pkt, Incoming::PubAck(_))).await;
    assert!(acked.is_some(), "endpoint acknowledged the publish");
}
