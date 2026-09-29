//! E2E — drive the real `run_cycle` poll loop through the embedded rumqttd broker.
//!
//! `lib-mqtt/tests/publish.rs` mocks the publisher, and `broker_roundtrip.rs` (in this crate)
//! hand-builds a `CanonicalReading` and publishes it directly through the broker link. Neither
//! exercises the actual pipeline that ships on the RPi: `impl_app::run_cycle` reading
//! a Seplos frame off the transport, mapping it to TMS, and publishing through the real
//! `BrokerPublisher`. This test drives that full path — `run_cycle` (scripted `MockSerial` fed the
//! real `LIVE_FRAME` capture, from `impl-app/tests/run_cycle.rs`) → `BrokerPublisher` → the
//! real embedded rumqttd broker → an external `rumqttc` subscriber — and asserts on what the
//! subscriber actually receives: publish count, exact topics, and the mapped JSON fields
//! (spot-checked against the known Seplos decode of `LIVE_FRAME`, see
//! `lib-parse/tests/seplos_vectors.rs`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use impl_app::{run_cycle, Health, Platform, RunConfig};
use lib_io::mock::{MockSerial, MockStep};
use lib_mapping::render::{render_fault, render_tms, TmsTopic};
use lib_mapping::schema::{topics, ClockMonotonic};
use lib_mapping::sink::publish_rendered;
use lib_mqtt::broker::BrokerPublisher;
use lib_parse::canonical::{CanonicalFault, CanonicalReading};
use lib_parse::seplos::Seplos;
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, Publish, QoS};

use test_kit::start_broker;

/// The live 14-cell telemetry capture (Seplos 48150-1101-10E, DIP 0) — same fixture as
/// `impl-app/tests/run_cycle.rs`. Known decode (see `seplos_vectors.rs`): 14 cells,
/// cell[0] = 3894 mV, current = -164 cA (-1.64 A, sign-inverted to +1.64 A on the TMS DC line),
/// total voltage = 5452 cV (54.52 V), soc = 775 dpct (0.775 p.u.).
const LIVE_FRAME: &[u8] = b"~20004600A08E00000E0F360F360F360F360F350F360F360F360F360F370F370F360F360F35060AF10AF90AF90AF30B370B0CFF5C154C36820A465003074650000203E8154CFFF800000079004DDEC8\r";

/// A `Platform` wiring the scripted transport to the real broker-backed publisher — the same
/// shape as `impl-app/tests/run_cycle.rs::TestPlatform`, but with `Pub = BrokerPublisher`
/// instead of an in-memory counter.
struct TestPlatform<'a, const N: usize> {
    transport: MockSerial<'a, N>,
    publisher: BrokerPublisher,
}
impl<'a, const N: usize> Platform for TestPlatform<'a, N> {
    type Transport = MockSerial<'a, N>;
    type Pub = BrokerPublisher;
    fn transport(&mut self) -> &mut Self::Transport {
        &mut self.transport
    }
    fn publisher(&mut self) -> &mut Self::Pub {
        &mut self.publisher
    }
    fn now(&self) -> ClockMonotonic {
        ClockMonotonic {
            seconds: 1000,
            nanoseconds: 0,
        }
    }
    async fn sleep(&mut self, _ms: u32) {}
    fn emit_reading(
        &mut self,
        cfg: &RunConfig,
        reading: &CanonicalReading,
        now: ClockMonotonic,
        payload_buf: &mut [u8],
        health: &mut Health,
    ) {
        let _ = publish_rendered(
            self.publisher(),
            &cfg.prefix,
            &cfg.device_id,
            topics::DC_MEASUREMENT_UPDATE,
            payload_buf,
            |buf| {
                render_tms(
                    TmsTopic::DcMeasurementUpdate,
                    cfg.device_id.clone(),
                    reading,
                    now,
                    buf,
                )
                .ok()
            },
        )
        .map(|()| health.record_publish(true))
        .map_err(|_| health.record_publish(false));
        let _ = publish_rendered(
            self.publisher(),
            &cfg.prefix,
            &cfg.device_id,
            topics::POWER_HARDWARE_UPDATE,
            payload_buf,
            |buf| {
                render_tms(
                    TmsTopic::PowerHardwareUpdate,
                    cfg.device_id.clone(),
                    reading,
                    now,
                    buf,
                )
                .ok()
            },
        )
        .map(|()| health.record_publish(true))
        .map_err(|_| health.record_publish(false));
    }
    fn emit_fault(
        &mut self,
        cfg: &RunConfig,
        fault: &CanonicalFault,
        now: ClockMonotonic,
        payload_buf: &mut [u8],
        health: &mut Health,
    ) {
        let _ = publish_rendered(
            self.publisher(),
            &cfg.prefix,
            &cfg.device_id,
            topics::ACTIVE_DIAGNOSTIC_STATE,
            payload_buf,
            |buf| render_fault(fault, cfg.device_id.clone(), now, buf).ok(),
        )
        .map(|()| health.record_publish(true))
        .map_err(|_| health.record_publish(false));
    }
}

fn one_dip_config() -> RunConfig {
    let mut device_id = heapless::String::new();
    device_id.push_str("METEOR-001").unwrap();
    let mut prefix = heapless::String::new();
    prefix.push_str("tms").unwrap();
    let mut dips = heapless::Vec::new();
    dips.push(0).unwrap();
    RunConfig {
        device_id,
        prefix,
        dip_addresses: dips,
        interval_ms: 2000,
        fault_every: 0,
        max_backoff_ms: 30_000,
    }
}

/// Drive the eventloop, collecting every `Incoming::Publish` until `want` have arrived or the
/// overall deadline elapses. Bounded — never an unbounded wait.
async fn collect_publishes(eventloop: &mut rumqttc::EventLoop, want: usize) -> Vec<Publish> {
    let deadline = Duration::from_secs(5);
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

/// Wait for the subscription SubAck before publishing (no retained messages here).
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

#[tokio::test]
async fn run_cycle_publishes_reach_the_broker_subscriber() {
    let (broker_guard, port) = start_broker();
    let mut broker = broker_guard;
    let publisher = broker.publisher();

    // External subscriber on the telemetry wildcard, same as a deployed mosquitto bridge.
    let mut opts = MqttOptions::new("test-e2e-sub", "127.0.0.1", port);
    opts.set_keep_alive(Duration::from_secs(5));
    let (client, mut eventloop) = AsyncClient::new(opts, 10);
    client
        .subscribe("tms/METEOR-001/#", QoS::AtLeastOnce)
        .await
        .expect("subscribe");
    wait_for_suback(&mut eventloop).await;

    // Drive the real poll loop: scripted transport delivers the live Seplos frame, the broker
    // Publisher pushes the two mapped TMS payloads onto the real rumqttd broker.
    let cfg = one_dip_config();
    let vendor = Seplos::new();
    let mut buf = [0u8; 256];
    let mut payload_buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    let mut health = Health::default();
    let mut plat: TestPlatform<4> = TestPlatform {
        transport: MockSerial::new(&[MockStep::Reply(LIVE_FRAME)]),
        publisher,
    };

    let outcome = run_cycle(
        &mut plat,
        &cfg,
        &vendor,
        &mut buf,
        &mut payload_buf,
        0,
        &mut health,
    )
    .await;
    assert!(outcome.any_success, "the live frame decodes cleanly");
    assert_eq!(health.telemetry_ok, 1);
    assert_eq!(
        health.publish_ok, 2,
        "DcMeasurementUpdate + PowerHardwareUpdate"
    );
    assert_eq!(health.publish_err, 0);
    assert!(broker.is_running());

    // The subscriber must receive exactly the two publishes, on the exact topics.
    let msgs = collect_publishes(&mut eventloop, 2).await;
    assert_eq!(
        msgs.len(),
        2,
        "exactly two publishes reached the subscriber"
    );

    let dc = msgs
        .iter()
        .find(|p| p.topic == "tms/METEOR-001/dc_measurement_update")
        .expect("DcMeasurementUpdate on the expected topic");
    let hw = msgs
        .iter()
        .find(|p| p.topic == "tms/METEOR-001/power_hardware_update")
        .expect("PowerHardwareUpdate on the expected topic");

    // DcMeasurementUpdate: sign-inverted amperage/voltage off the live frame's DC line.
    let dc_json: serde_json::Value = serde_json::from_slice(&dc.payload).expect("valid TMS json");
    assert_eq!(dc_json["device_id"], "METEOR-001");
    let amps = dc_json["external_measurement"][0]["line"][0]["amperage"]
        .as_f64()
        .unwrap();
    assert!(
        (amps - 1.64).abs() < 0.01,
        "discharging -1.64 A maps to TMS +1.64 A, got {amps}"
    );
    let volts = dc_json["external_measurement"][0]["line"][0]["voltage"]
        .as_f64()
        .unwrap();
    assert!((volts - 54.52).abs() < 0.01, "54.52 V, got {volts}");

    // PowerHardwareUpdate: energy_storage carries all 14 decoded cells.
    let hw_json: serde_json::Value = serde_json::from_slice(&hw.payload).expect("valid TMS json");
    assert_eq!(hw_json["device_id"], "METEOR-001");
    let cells = hw_json["energy_storage"]["storage_cells"]
        .as_array()
        .expect("storage_cells array");
    assert_eq!(cells.len(), 14, "14 decoded cells");
    let cell0_v = cells[0]["voltage"].as_f64().unwrap();
    assert!(
        (cell0_v - 3.894).abs() < 0.001,
        "cell[0] = 3894 mV, got {cell0_v}"
    );
    let soc = cells[0]["state_of_charge"].as_f64().unwrap();
    assert!(
        (soc - 0.775).abs() < 0.001,
        "775 dpct -> 0.775 p.u., got {soc}"
    );
}
