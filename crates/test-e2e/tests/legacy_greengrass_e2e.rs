//! Legacy AWS Greengrass backwards-compat route — renderer level (Scope D, closes #52-#54,
//! reconciled against the real baseline — gap-closure #269).
//!
//! Boots a real embedded broker (the same `LocalBroker` `bin-rpi` embeds), builds a real
//! `bin_rpi_platform::GreengrassTap` over its own [`test_kit`]-minted `BrokerPublisher`, offers it
//! golden `CanonicalReading`/`CanonicalFault` events via `TapEvent`, and asserts an external
//! `rumqttc` subscriber on `telemetry/#` receives the exact merged flat-JSON battery payload on the
//! exact `telemetry/{device_type}/{device_id}` topic — the same path `distro/deploy/mosquitto.conf`'s
//! Bridge 3 relays out to a real AWS Greengrass core.
//!
//! **BASELINE-VERIFIED**: the merged flat JSON field set/scale-factors asserted here mirror
//! `lib_mapping::legacy::battery_payload`'s baseline-verified shape (`/mnt/d/hvd/meteor-adapter`'s
//! `example/aws/src/greengrass/telemetry/battery/{battery_reader.py,config.yaml}` and
//! `common/device_telemetry_base.py` — see that function's own doc comment).
//!
//! **Fault-merge cache (D2, gap-closure #269)**: a `TapEvent::Fault` updates the tap's per-DIP
//! fault cache and publishes nothing; the flags land in the SAME payload as the NEXT `Reading` for
//! that DIP — the baseline's merge-on-success behavior. `greengrass_tap_merges_cached_fault_into_...`
//! pins that. These are RENDERER-level tests (they call `GreengrassTap::offer` directly); the full
//! `HostTap::offer` dispatch through a real `SinkSet` + `run_cycle` is proven in
//! `greengrass_sinkset_e2e.rs`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use bin_rpi_platform::{GreengrassTap, TapEvent};
use lib_mapping::schema::ClockMonotonic;
use lib_parse::canonical::{CanonicalFault, CanonicalReading};
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};

use test_kit::start_broker;

/// Matches `broker_roundtrip.rs`'s golden fixture (T8) so both bridges' tests exercise the same
/// canonical reading.
fn sample_reading() -> CanonicalReading {
    let mut cells = heapless::Vec::new();
    cells.push(3894).unwrap();
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

fn sample_fault() -> CanonicalFault {
    CanonicalFault {
        device_address: 0,
        high_voltage_protection: true,
        mosfet_fault: true,
        ..Default::default()
    }
}

fn a_clock() -> ClockMonotonic {
    ClockMonotonic {
        seconds: 1_700_000_000,
        nanoseconds: 0,
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

async fn subscribe_telemetry(client: &AsyncClient, eventloop: &mut rumqttc::EventLoop) {
    client
        .subscribe("telemetry/#", QoS::AtLeastOnce)
        .await
        .expect("subscribe");
    let acked = pump_until(eventloop, |pkt| matches!(pkt, Incoming::SubAck(_))).await;
    assert!(acked.is_some(), "broker acknowledged the subscription");
}

#[tokio::test]
async fn greengrass_tap_publishes_merged_flat_battery_json() {
    let (mut broker, port) = start_broker();
    // GreengrassTap's OWN publisher handle — a second `LocalBroker::publisher()` clone, distinct
    // from any primary TMS publisher (see `GreengrassTap`'s doc comment).
    let publisher = broker.publisher();
    // No `[location]` chain configured for this test — `run_refresher`'s `tx` side is never wired,
    // so the receiver just observes its initial `None` forever.
    let (_loc_tx, loc_rx) = tokio::sync::watch::channel(None);
    let mut tap = GreengrassTap::new(publisher, "METEOR-001", "battery", loc_rx);

    let mut opts = MqttOptions::new("sentient-legacy-sub", "127.0.0.1", port);
    opts.set_keep_alive(Duration::from_secs(5));
    let (client, mut eventloop) = AsyncClient::new(opts, 10);
    subscribe_telemetry(&client, &mut eventloop).await;

    let reading = sample_reading();
    tap.offer(&TapEvent::Reading {
        reading: &reading,
        now: a_clock(),
        bus: "bus0",
    });

    let got = pump_until(&mut eventloop, |pkt| matches!(pkt, Incoming::Publish(_))).await;
    let publish = match got {
        Some(Incoming::Publish(p)) => p,
        _ => panic!("did not receive the greengrass tap's telemetry publish"),
    };

    // Baseline topic namespace: telemetry/{device_type}/{device_id} — no `/kind` suffix (#269).
    assert_eq!(publish.topic, "telemetry/battery/METEOR-001");

    let v: serde_json::Value = serde_json::from_slice(&publish.payload).expect("valid legacy json");

    // `device_id` is the tap's OWN configured identity, not a per-frame address derivation.
    assert_eq!(v["device_id"], "METEOR-001");
    assert_eq!(v["device_type"], "battery");
    assert_eq!(v["dip_address"], 0);
    let timestamp = v["timestamp"].as_str().expect("timestamp string");
    assert!(timestamp.ends_with('Z'), "timestamp: {timestamp}");

    // No location chain configured -> latitude/longitude KEYS OMITTED entirely (baseline-exact;
    // not emitted as JSON null), "unknown" source.
    let obj = v.as_object().expect("payload is a JSON object");
    assert!(
        !obj.contains_key("latitude"),
        "latitude key must be omitted"
    );
    assert!(
        !obj.contains_key("longitude"),
        "longitude key must be omitted"
    );
    assert_eq!(v["location_source"], "unknown");

    assert_eq!(v["num_cells"], 1);
    assert_eq!(v["cell_0_voltage"], 3894);

    let current = v["current"].as_f64().expect("current number");
    assert!((current - (-1.44)).abs() < 0.01, "current: {current}");
    let residual = v["residual_capacity"].as_f64().expect("residual number");
    assert!(
        (residual - 139.53).abs() < 0.01,
        "residual_capacity: {residual}"
    );
    let battery_capacity = v["battery_capacity"].as_f64().expect("capacity number");
    assert!(
        (battery_capacity - 180.0).abs() < 0.01,
        "battery_capacity: {battery_capacity}"
    );
    let soc = v["state_of_charge"].as_f64().expect("soc number");
    assert!((soc - 77.5).abs() < 0.01, "state_of_charge: {soc}");
    let soh = v["state_of_health"].as_f64().expect("soh number");
    assert!((soh - 100.0).abs() < 0.01, "state_of_health: {soh}");
    assert_eq!(v["num_cycles"], 2);
    let port_voltage = v["port_voltage"].as_f64().expect("port_voltage number");
    assert!(
        (port_voltage - 54.53).abs() < 0.01,
        "port_voltage: {port_voltage}"
    );

    // Baseline-disabled fields never appear.
    assert!(v.get("total_voltage").is_none());
    assert!(v.get("rated_capacity").is_none());
    assert!(v.get("custom_number").is_none());

    // No fault polled yet -> no fault-flag keys at all (not even `0`).
    assert!(v.get("high_voltage_protection").is_none());
    assert!(v.get("mosfet_fault").is_none());

    assert!(broker.is_running());
}

/// D2 fault-merge cache: a `Fault` event caches the flags and publishes nothing; the NEXT `Reading`
/// for that DIP renders them into the SAME merged payload (baseline merge-on-success).
#[tokio::test]
async fn greengrass_tap_merges_cached_fault_into_next_reading() {
    let (mut broker, port) = start_broker();
    let publisher = broker.publisher();
    let (_loc_tx, loc_rx) = tokio::sync::watch::channel(None);
    let mut tap = GreengrassTap::new(publisher, "METEOR-001", "battery", loc_rx);

    let mut opts = MqttOptions::new("sentient-legacy-merge-sub", "127.0.0.1", port);
    opts.set_keep_alive(Duration::from_secs(5));
    let (client, mut eventloop) = AsyncClient::new(opts, 10);
    subscribe_telemetry(&client, &mut eventloop).await;

    // A bare Fault caches only — it must NOT publish. Then a Reading publishes the merged payload.
    let fault = sample_fault();
    tap.offer(&TapEvent::Fault {
        fault: &fault,
        now: a_clock(),
        bus: "bus0",
    });
    let reading = sample_reading();
    tap.offer(&TapEvent::Reading {
        reading: &reading,
        now: a_clock(),
        bus: "bus0",
    });

    // The FIRST publish to arrive must be the reading (proving the Fault published nothing ahead of
    // it) — and it must carry the merged fault flags from the cache.
    let got = pump_until(&mut eventloop, |pkt| matches!(pkt, Incoming::Publish(_))).await;
    let publish = match got {
        Some(Incoming::Publish(p)) => p,
        _ => panic!("expected the merged reading publish"),
    };
    assert_eq!(
        publish.topic, "telemetry/battery/METEOR-001",
        "a bare Fault must not publish its own message; the reading is the first (and only) publish"
    );

    let v: serde_json::Value = serde_json::from_slice(&publish.payload).expect("valid legacy json");
    // Merged fault flags present, with the set bits as 1 and every other flag as an explicit 0.
    assert_eq!(v["high_voltage_protection"], 1);
    assert_eq!(v["mosfet_fault"], 1);
    assert_eq!(v["low_voltage_protection"], 0);
    assert_eq!(v["short_circuit_protection"], 0);
    assert_eq!(v["charge_current_warning"], 0);

    assert!(broker.is_running());
}

/// A disabled tap (`enabled: false`) must never publish — the runtime kill-switch works even with
/// a live broker/publisher wired up.
#[tokio::test]
async fn disabled_greengrass_tap_publishes_nothing() {
    let (mut broker, port) = start_broker();
    let publisher = broker.publisher();
    let (_loc_tx, loc_rx) = tokio::sync::watch::channel(None);
    let mut tap = GreengrassTap::new(publisher, "METEOR-001", "battery", loc_rx);
    tap.enabled = false;

    let mut opts = MqttOptions::new("sentient-legacy-disabled-sub", "127.0.0.1", port);
    opts.set_keep_alive(Duration::from_secs(5));
    let (client, mut eventloop) = AsyncClient::new(opts, 10);
    subscribe_telemetry(&client, &mut eventloop).await;

    let reading = sample_reading();
    tap.offer(&TapEvent::Reading {
        reading: &reading,
        now: a_clock(),
        bus: "bus0",
    });

    // Publish a canary on a distinct topic through the same broker so we have a positive signal
    // the eventloop is actually pumping (rather than the absence just meaning "never polled").
    let mut publisher2 = broker.publisher();
    use lib_mapping::sink::Publisher;
    publisher2
        .publish("telemetry/canary", b"x")
        .expect("canary publish");

    let got = pump_until(&mut eventloop, |pkt| matches!(pkt, Incoming::Publish(_))).await;
    let publish = match got {
        Some(Incoming::Publish(p)) => p,
        _ => panic!("expected at least the canary publish to arrive"),
    };
    assert_eq!(
        publish.topic, "telemetry/canary",
        "the disabled tap must not have published a reading ahead of the canary"
    );
}
