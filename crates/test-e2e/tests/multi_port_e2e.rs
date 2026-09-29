//! Multi-port (#238 fan-out, #242 multi-port) — TWO independent serial buses, each with its own
//! real pty-backed `SeplosSim` responder and its own `bin_rpi_platform::HostPlatform` (the REAL
//! production seam, not a hand-rolled `Platform` test double — see `e2e_l1.rs`'s doc comment for
//! why that file uses one), sharing ONE embedded broker via `LocalBroker::publisher()` minted
//! twice. Proves: (a) `HostPlatform::build_port` composes with a caller-owned, externally-shared
//! broker exactly as `bin-rpi::runtime::supervise` uses it; (b) two concurrent poll cycles over
//! two independent transports interleave their publishes onto the one shared broker without
//! cross-bus interference; (c) both buses' telemetry (distinguishable by payload content, since
//! both buses publish under the same device identity's topic set — see `HostPlatform`'s doc
//! comment on why multi-port shares one `Identity`) reaches a single external subscriber.
//!
//! Mirrors `e2e_l1.rs`'s pty/responder harness (byte-for-byte the same helpers, duplicated here
//! per this crate's existing per-file convention — see `host_pty.rs`/`e2e_l1.rs`, each carrying
//! their own copy rather than sharing a helper module).
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::thread;
use std::time::Duration;

use nix::fcntl::OFlag;
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt, PtyMaster};
use serial_test::file_serial;

use bin_rpi_platform::{CyclePulse, DirectionMode, HostPlatform, PreflightInputs, SinkSet};
use impl_app::{run_cycle, Health, RunConfig};
use lib_io::host::SerialKind;
use lib_io::transport::SerialParams;
use lib_parse::seplos::Seplos;
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, Publish, QoS};
use test_kit::{start_broker, BatteryState, Behavior, SeplosSim, SimReply};

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

/// Spawn the `SeplosSim` responder on its own thread: read one command frame off the pty master
/// and answer it via `sim.respond` (one exchange only — one telemetry poll per bus per cycle).
fn spawn_sim_responder(master: PtyMaster, mut sim: SeplosSim) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut f = master_file(&master);
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
        if !cmd.is_empty() {
            match sim.respond(&cmd) {
                SimReply::Bytes(bytes) => {
                    let _ = f.write_all(bytes.as_slice());
                    let _ = f.flush();
                }
                SimReply::Silence => {}
            }
        }
        thread::sleep(Duration::from_millis(150));
        std::mem::forget(f);
        drop(master);
    })
}

fn test_preflight_inputs() -> PreflightInputs {
    PreflightInputs {
        device_id_present: true,
        dip_addresses_present: true,
        cloud: None,
    }
}

fn one_dip_config(device_id: &str) -> RunConfig {
    let mut id = heapless::String::new();
    id.push_str(device_id).unwrap();
    let mut prefix = heapless::String::new();
    prefix.push_str("tms").unwrap();
    let mut dips = heapless::Vec::new();
    dips.push(0).unwrap();
    RunConfig {
        device_id: id,
        prefix,
        dip_addresses: dips,
        interval_ms: 2000,
        fault_every: 0,
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
/// overall deadline elapses.
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

/// Two buses, one broker: both poll cycles run concurrently, and every publish (3 TMS topics per
/// successful exchange — `DcMeasurementUpdate`/`PowerHardwareUpdate`/`StorageUpdate`, see
/// `HostPlatform::emit_reading`) from BOTH buses lands on the one shared subscriber.
#[tokio::test]
#[file_serial(seplos_e2e)]
async fn two_buses_share_one_broker_and_interleave_publishes() {
    // Bus A: the stock 14-cell sample. Bus B: the same shape but a distinguishable total voltage,
    // so the two buses' `PowerHardwareUpdate`/`StorageUpdate` payloads are tellable apart.
    let (master_a, slave_a) = open_pty();
    let state_a = BatteryState::sample_14_cell();
    let sim_a = SeplosSim::new(state_a);
    let responder_a = spawn_sim_responder(master_a, sim_a);

    let (master_b, slave_b) = open_pty();
    let mut state_b = BatteryState::sample_14_cell();
    state_b.total_voltage_cv = 6000; // deliberately different from bus A
    let mut sim_b = SeplosSim::new(state_b);
    sim_b.set_behavior(Behavior::Normal);
    let responder_b = spawn_sim_responder(master_b, sim_b);

    let (broker_guard, port) = start_broker();
    let mut broker = broker_guard;

    let mut opts = MqttOptions::new("test-e2e-multi-port-sub", "127.0.0.1", port);
    opts.set_keep_alive(Duration::from_secs(5));
    let (client, mut eventloop) = AsyncClient::new(opts, 10);
    client
        .subscribe("tms/METEOR-MULTI/#", QoS::AtLeastOnce)
        .await
        .expect("subscribe");
    wait_for_suback(&mut eventloop).await;

    let platform_a = HostPlatform::build_port(
        broker.publisher(),
        SinkSet::default(),
        "bus-a".to_string(),
        &slave_a,
        SerialKind::Rs232,
        SerialParams::default(),
        DirectionMode::default(),
        400,
        CyclePulse::default(),
        test_preflight_inputs(),
        None,
        Duration::from_secs(10),
    )
    .expect("build_port bus-a");
    let platform_b = HostPlatform::build_port(
        broker.publisher(),
        SinkSet::default(),
        "bus-b".to_string(),
        &slave_b,
        SerialKind::Rs232,
        SerialParams::default(),
        DirectionMode::default(),
        400,
        CyclePulse::default(),
        test_preflight_inputs(),
        None,
        Duration::from_secs(10),
    )
    .expect("build_port bus-b");

    let cfg = one_dip_config("METEOR-MULTI");
    let vendor = Seplos::new();

    async fn run_one(
        mut platform: HostPlatform,
        cfg: RunConfig,
        vendor: &Seplos,
    ) -> (bool, Health) {
        let mut buf = [0u8; 256];
        let mut payload_buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
        let mut health = Health::default();
        let outcome = run_cycle(
            &mut platform,
            &cfg,
            vendor,
            &mut buf,
            &mut payload_buf,
            0,
            &mut health,
        )
        .await;
        (outcome.any_success, health)
    }

    let (a_result, b_result) = tokio::join!(
        run_one(platform_a, cfg.clone(), &vendor),
        run_one(platform_b, cfg.clone(), &vendor),
    );

    assert!(a_result.0, "bus A's sim-encoded frame must decode cleanly");
    assert!(b_result.0, "bus B's sim-encoded frame must decode cleanly");
    assert_eq!(
        a_result.1.publish_ok, 3,
        "bus A: DcMeasurementUpdate + PowerHardwareUpdate + StorageUpdate"
    );
    assert_eq!(
        b_result.1.publish_ok, 3,
        "bus B: DcMeasurementUpdate + PowerHardwareUpdate + StorageUpdate"
    );
    assert!(broker.is_running(), "the shared broker stays up");

    let msgs = collect_publishes(&mut eventloop, 6).await;
    assert_eq!(
        msgs.len(),
        6,
        "exactly six publishes (3 topics x 2 buses) reached the subscriber"
    );

    let dc_count = msgs
        .iter()
        .filter(|p| p.topic == "tms/METEOR-MULTI/dc_measurement_update")
        .count();
    let hw_count = msgs
        .iter()
        .filter(|p| p.topic == "tms/METEOR-MULTI/power_hardware_update")
        .count();
    let storage_count = msgs
        .iter()
        .filter(|p| p.topic == "tms/METEOR-MULTI/storage_update")
        .count();
    assert_eq!(dc_count, 2, "one DcMeasurementUpdate per bus");
    assert_eq!(hw_count, 2, "one PowerHardwareUpdate per bus");
    assert_eq!(storage_count, 2, "one StorageUpdate per bus");

    // Bus A and bus B were seeded with distinct `total_voltage_cv` — their two
    // `PowerHardwareUpdate` payloads must therefore differ, proving this is genuinely two
    // independent exchanges interleaved onto one broker, not one bus's output published twice.
    let hw_payloads: Vec<&[u8]> = msgs
        .iter()
        .filter(|p| p.topic == "tms/METEOR-MULTI/power_hardware_update")
        .map(|p| p.payload.as_ref())
        .collect();
    assert_eq!(hw_payloads.len(), 2);
    assert_ne!(
        hw_payloads[0], hw_payloads[1],
        "bus A's and bus B's PowerHardwareUpdate payloads must differ (distinct total_voltage_cv)"
    );

    responder_a.join().ok();
    responder_b.join().ok();
}
