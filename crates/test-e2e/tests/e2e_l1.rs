//! L1 — the full pipeline over a REAL pty: `SeplosSim` (pure, transport-agnostic responder) bound
//! to a pty → the real `HostSerial` (`tokio-serial`) transport → `impl_app::run_cycle`
//! (`driver::read_telemetry`/`read_fault`) → the TMS map → the real embedded rumqttd broker → an
//! external `rumqttc` subscriber.
//!
//! This is the FOUNDATION e2e layer (L1): everything downstream of `SeplosSim` is the actual
//! shipping code path (no mocks past the wire), and everything upstream of the driver's `\r` read
//! is a real OS pty — the same transport shape as production RS-232/RS-485, minus the physical
//! line. `host_pty.rs` (in this crate) remains the transport-fault oracle for `HostSerial`
//! itself (raw byte vectors); this file is the pipeline oracle (Seplos frame → TMS → broker).
//!
//! `MockSerial`-driven tests (`impl-app/tests/run_cycle.rs`, `mqtt_e2e.rs`) stay separate on
//! purpose — `MockSerial::read_until` does not exercise real `\r`-delimiter framing, so they are
//! not a substitute for this pty path (see `lib-io/src/mock.rs`).
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::thread;
use std::time::Duration;

use nix::fcntl::OFlag;
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt, PtyMaster};
use serial_test::file_serial;

use impl_app::{run_cycle, Health, Platform, RunConfig};
use lib_io::host::{HostSerial, SerialKind};
use lib_io::transport::SerialParams;
use lib_mapping::render::{render_fault, render_tms, TmsTopic};
use lib_mapping::schema::{topics, ClockMonotonic};
use lib_mapping::sink::publish_rendered;
use lib_mqtt::broker::BrokerPublisher;
use lib_parse::canonical::{CanonicalFault, CanonicalReading};
use lib_parse::seplos::Seplos;
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, Publish, QoS};
use test_kit::{start_broker, BatteryState, Behavior, FaultFlags, SeplosSim, SimReply};

/// Open a pty pair; return the master handle and the slave device path (`/dev/pts/N`).
fn open_pty() -> (PtyMaster, String) {
    let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).expect("posix_openpt");
    grantpt(&master).expect("grantpt");
    unlockpt(&master).expect("unlockpt");
    // SAFETY (test-only): single-threaded bring-up; the path is used immediately.
    let slave = unsafe { ptsname(&master) }.expect("ptsname");
    (master, slave)
}

/// Borrow the master fd as a `File` without owning it (`PtyMaster` keeps ownership).
fn master_file(master: &PtyMaster) -> std::fs::File {
    use std::os::unix::io::FromRawFd;
    // SAFETY (test-only): the returned File is forgotten by the caller, so the fd is not closed.
    unsafe { std::fs::File::from_raw_fd(master.as_raw_fd()) }
}

/// Spawn the `SeplosSim` responder on its own thread: read up to `exchanges` command frames off
/// the pty master (bounded byte-at-a-time reads, mirroring `lib-io/tests/host_pty.rs`), and
/// answer each via `sim.respond`. `long_hold` keeps the master open well past the driver's read
/// deadline for behaviors that must genuinely time out (`Silence`/`Truncated`) rather than see an
/// early EOF (which would surface as a *different* transport error).
fn spawn_sim_responder(
    master: PtyMaster,
    mut sim: SeplosSim,
    exchanges: usize,
    long_hold: bool,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut f = master_file(&master);
        for _ in 0..exchanges {
            let mut cmd = Vec::new();
            let mut byte = [0u8; 1];
            for _ in 0..64 {
                match f.read(&mut byte) {
                    Ok(1) => {
                        cmd.push(byte[0]);
                        if byte[0] == b'\r' {
                            break;
                        }
                    }
                    _ => break,
                }
            }
            if cmd.is_empty() {
                break;
            }
            match sim.respond(&cmd) {
                SimReply::Bytes(bytes) => {
                    let _ = f.write_all(bytes.as_slice());
                    let _ = f.flush();
                }
                SimReply::Silence => {}
            }
        }
        let hold_ms = if long_hold { 700 } else { 150 };
        thread::sleep(Duration::from_millis(hold_ms));
        std::mem::forget(f);
        drop(master);
    })
}

/// Open the real host transport against the pty slave with a bounded read deadline. RS-232 (not
/// RS-485) because a pty has no RTS modem line for half-duplex direction control — the
/// request/response mechanics under test are identical either way (same convention as
/// `lib-io/tests/host_pty.rs`).
fn open_transport(slave: &str) -> HostSerial {
    HostSerial::open(slave, SerialKind::Rs232, SerialParams::default(), 400)
        .expect("open pty slave as serial")
}

/// A `Platform` wiring the REAL pty-backed `HostSerial` transport to the REAL broker-backed
/// publisher — the L1 harness's whole point. Same shape as `mqtt_e2e.rs::TestPlatform`, but
/// `Transport = HostSerial` instead of `MockSerial`.
struct TestPlatform {
    transport: HostSerial,
    publisher: BrokerPublisher,
}
impl Platform for TestPlatform {
    type Transport = HostSerial;
    type Pub = BrokerPublisher;
    fn transport(&mut self) -> &mut Self::Transport {
        &mut self.transport
    }
    fn publisher(&mut self) -> &mut Self::Pub {
        &mut self.publisher
    }
    fn now(&self) -> ClockMonotonic {
        // Fixed (not wall-clock) so payloads — and the insta snapshots below — are deterministic.
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

fn one_dip_config(fault_every: u32) -> RunConfig {
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
        fault_every,
        max_backoff_ms: 30_000,
    }
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

/// Drive the eventloop, collecting every `Incoming::Publish` until `want` have arrived or the
/// overall deadline elapses. Bounded — never an unbounded wait; used for both the "N publishes
/// arrive" and the "zero publishes arrive" (degraded outcome) assertions.
async fn collect_publishes(eventloop: &mut rumqttc::EventLoop, want: usize) -> Vec<Publish> {
    let deadline = Duration::from_secs(3);
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

/// Common bring-up: pty + sim responder + real broker + subscriber on the telemetry wildcard.
/// Returns the joined pieces the caller drives `run_cycle` over.
async fn bring_up(
    sim_state: BatteryState,
    behavior: Behavior,
    exchanges: usize,
    long_hold: bool,
) -> (
    thread::JoinHandle<()>,
    TestPlatform,
    lib_mqtt::broker::LocalBroker,
    AsyncClient,
    rumqttc::EventLoop,
) {
    let (master, slave) = open_pty();
    let mut sim = SeplosSim::new(sim_state);
    sim.set_behavior(behavior);
    let responder = spawn_sim_responder(master, sim, exchanges, long_hold);

    let (broker_guard, port) = start_broker();
    let mut broker = broker_guard;
    let publisher = broker.publisher();

    let mut opts = MqttOptions::new("test-e2e-l1-sub", "127.0.0.1", port);
    opts.set_keep_alive(Duration::from_secs(5));
    let (client, mut eventloop) = AsyncClient::new(opts, 10);
    client
        .subscribe("tms/METEOR-001/#", QoS::AtLeastOnce)
        .await
        .expect("subscribe");
    wait_for_suback(&mut eventloop).await;

    let plat = TestPlatform {
        transport: open_transport(&slave),
        publisher,
    };
    (responder, plat, broker, client, eventloop)
}

#[tokio::test]
#[file_serial(seplos_e2e)]
async fn good_telemetry_publishes_two_tms_messages_matching_snapshot() {
    let (responder, mut plat, broker, _client, mut eventloop) =
        bring_up(BatteryState::sample_14_cell(), Behavior::Normal, 1, false).await;

    let cfg = one_dip_config(0);
    let vendor = Seplos::new();
    let mut buf = [0u8; 256];
    let mut payload_buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    let mut health = Health::default();

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
    assert!(
        outcome.any_success,
        "the sim-encoded frame must decode cleanly"
    );
    assert_eq!(health.telemetry_ok, 1);
    assert_eq!(health.telemetry_err, 0);
    assert_eq!(
        health.publish_ok, 2,
        "DcMeasurementUpdate + PowerHardwareUpdate"
    );
    assert_eq!(health.publish_err, 0);
    assert!(broker.is_running());

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

    // sort_maps: snapshot ORDER-INSENSITIVELY. serde_json's map order here depends on cargo
    // feature unification (lib-mapping's dev-only `preserve_order` bleeds into this binary under
    // `cargo test --workspace` but not under `just test-e2e`), so an order-sensitive snapshot
    // flips between invocation styles. Field ORDER is pinned where it belongs — the wire goldens
    // and the cross-oracle in lib-mapping — this snapshot pins VALUES and shape.
    let dc_json: serde_json::Value = serde_json::from_slice(&dc.payload).expect("valid TMS json");
    insta::with_settings!({sort_maps => true}, {
        insta::assert_json_snapshot!("l1_good_telemetry_dc_measurement", dc_json, {
            ".time_measured" => "[time]",
        });
    });

    let hw_json: serde_json::Value = serde_json::from_slice(&hw.payload).expect("valid TMS json");
    insta::with_settings!({sort_maps => true}, {
        insta::assert_json_snapshot!("l1_good_telemetry_power_hardware", hw_json, {
            ".timestamp" => "[time]",
        });
    });

    responder.join().ok();
}

#[tokio::test]
#[file_serial(seplos_e2e)]
async fn good_telemetry_and_fault_publishes_three_tms_messages_matching_snapshot() {
    let mut state = BatteryState::sample_14_cell();
    state.faults = FaultFlags {
        high_voltage_protection: true,
        short_circuit_protection: true,
        ..Default::default()
    };
    let (responder, mut plat, broker, _client, mut eventloop) =
        bring_up(state, Behavior::Normal, 2, false).await;

    let cfg = one_dip_config(1); // poll the fault command every cycle
    let vendor = Seplos::new();
    let mut buf = [0u8; 256];
    let mut payload_buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    let mut health = Health::default();

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
    assert!(outcome.any_success);
    assert_eq!(health.telemetry_ok, 1);
    assert_eq!(health.fault_ok, 1);
    assert_eq!(health.fault_err, 0);
    assert_eq!(
        health.publish_ok, 3,
        "DcMeasurementUpdate + PowerHardwareUpdate + ActiveDiagnosticState"
    );
    assert!(broker.is_running());

    let msgs = collect_publishes(&mut eventloop, 3).await;
    assert_eq!(
        msgs.len(),
        3,
        "exactly three publishes reached the subscriber"
    );

    let diag = msgs
        .iter()
        .find(|p| p.topic == "tms/METEOR-001/active_diagnostic_state")
        .expect("ActiveDiagnosticState on the expected topic");
    let diag_json: serde_json::Value =
        serde_json::from_slice(&diag.payload).expect("valid TMS json");
    let codes = diag_json["codes"].as_array().expect("codes array");
    assert_eq!(codes.len(), 2, "exactly the two set fault flags");
    let hints: Vec<&str> = codes
        .iter()
        .map(|c| c["hint"].as_str().expect("hint"))
        .collect();
    assert!(hints.contains(&"cell overvoltage"));
    assert!(hints.contains(&"short circuit"));

    insta::with_settings!({sort_maps => true}, {
        insta::assert_json_snapshot!("l1_good_fault_active_diagnostic_state", diag_json, {
            ".codes[].time_raised" => "[time]",
        });
    });

    responder.join().ok();
}

/// One degraded-outcome case: `behavior` must never crash the pipeline, must count the failure,
/// and must publish nothing.
async fn assert_degrades_gracefully(behavior: Behavior, long_hold: bool) {
    let (responder, mut plat, broker, _client, mut eventloop) =
        bring_up(BatteryState::sample_14_cell(), behavior, 1, long_hold).await;

    let cfg = one_dip_config(0);
    let vendor = Seplos::new();
    let mut buf = [0u8; 256];
    let mut payload_buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    let mut health = Health::default();

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

    assert!(
        !outcome.any_success,
        "behavior {behavior:?} must degrade, never silently succeed"
    );
    assert_eq!(health.telemetry_ok, 0);
    assert_eq!(health.telemetry_err, 1);
    assert_eq!(health.publish_ok, 0);
    assert_eq!(health.consecutive_failed_cycles, 1);
    assert!(
        broker.is_running(),
        "the broker itself must stay up through a device fault"
    );

    let msgs = collect_publishes(&mut eventloop, 1).await;
    assert!(msgs.is_empty(), "a degraded exchange must publish nothing");

    responder.join().ok();
}

#[tokio::test]
#[file_serial(seplos_e2e)]
async fn behavior_silence_degrades_gracefully() {
    assert_degrades_gracefully(Behavior::Silence, true).await;
}

#[tokio::test]
#[file_serial(seplos_e2e)]
async fn behavior_corrupt_checksum_degrades_gracefully() {
    assert_degrades_gracefully(Behavior::CorruptChecksum, false).await;
}

#[tokio::test]
#[file_serial(seplos_e2e)]
async fn behavior_wrong_address_degrades_gracefully() {
    assert_degrades_gracefully(Behavior::WrongAddress, false).await;
}

#[tokio::test]
#[file_serial(seplos_e2e)]
async fn behavior_truncated_degrades_gracefully() {
    assert_degrades_gracefully(Behavior::Truncated(10), true).await;
}

#[tokio::test]
#[file_serial(seplos_e2e)]
async fn behavior_garbage_degrades_gracefully() {
    assert_degrades_gracefully(Behavior::Garbage, false).await;
}
