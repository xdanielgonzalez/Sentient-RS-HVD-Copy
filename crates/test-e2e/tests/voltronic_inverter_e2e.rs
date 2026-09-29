//! The load-bearing proof for the RS-232 Voltronic inverter leg (D9/D10/D10b, decision 0001).
//!
//! Drives the REAL `impl_app::run_cycle_inverter` poll loop over a REAL pty-backed
//! [`VoltronicSim`] responder and a REAL embedded broker, through the REAL
//! `bin_rpi_platform::HostPlatform` built via `HostPlatform::build_port` with a REAL [`SinkSet`]
//! carrying a REAL `HostTap::Greengrass(GreengrassTap)` (`device_type = "inverter"`). It asserts the
//! full CONV pipeline reaches a subscriber:
//!
//! * the four inverter TMS topics on `tms/{inv-id}/*` — `ac_measurement_update`,
//!   `dc_measurement_update`, `power_hardware_update`, `active_diagnostic_state` — mapped by
//!   `lib_mapping::transform_inverter` and published by `HostPlatform::emit_inverter_reading` /
//!   `emit_inverter_fault`, AND
//! * the merged flat legacy inverter payload on `telemetry/inverter/{inv-id}`, rendered by
//!   `lib_mapping::legacy::inverter_payload` via the `GreengrassTap`'s `InverterReading` arm.
//!
//! Fault-merge (D2, inverter): the QPIWS overload fault set on the sim caches on cycle 0's
//! `InverterFault` (emitted AFTER cycle 0's reading), so cycle 0's legacy payload carries NO fault
//! flags and cycle 1's carries them — proving the inverter fault cache persists across cycles, the
//! same shape the battery path proves in `greengrass_sinkset_e2e.rs`.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::thread;
use std::time::Duration;

use nix::fcntl::OFlag;
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt, PtyMaster};
use serial_test::file_serial;

use bin_rpi_platform::{
    CyclePulse, DirectionMode, GreengrassTap, HostPlatform, HostTap, PreflightInputs, SinkSet,
};
use impl_app::{run_cycle_inverter, Health, RunConfig};
use lib_io::host::SerialKind;
use lib_io::transport::SerialParams;
use lib_parse::canonical::InverterRatings;
use lib_parse::voltronic::Voltronic;
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, Publish, QoS};
use test_kit::{start_broker, VoltronicSim};

const INV_ID: &str = "METEOR-001-inv";

fn open_pty() -> (PtyMaster, String) {
    let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).expect("posix_openpt");
    grantpt(&master).expect("grantpt");
    unlockpt(&master).expect("unlockpt");
    // SAFETY (test-only): single-threaded bring-up; the path is used immediately.
    let slave = unsafe { ptsname(&master) }.expect("ptsname");
    (master, slave)
}

fn master_file(master: &PtyMaster) -> std::fs::File {
    use std::os::unix::io::FromRawFd;
    // SAFETY (test-only): the returned File is forgotten by the caller, so the fd is not closed.
    unsafe { std::fs::File::from_raw_fd(master.as_raw_fd()) }
}

/// Read one command frame (bytes up to and including `\r`) off the pty master. None of the Voltronic
/// request-command CRCs contain `0x0D` (verified), so this reads a clean, whole request frame.
fn read_command(f: &mut std::fs::File) -> Option<Vec<u8>> {
    let mut cmd = Vec::new();
    let mut byte = [0u8; 1];
    for _ in 0..32 {
        match f.read(&mut byte) {
            Ok(1) => {
                cmd.push(byte[0]);
                if byte[0] == b'\r' {
                    return Some(cmd);
                }
            }
            _ => break,
        }
    }
    None
}

/// Spawn a LOOPING `VoltronicSim` responder: read `exchanges` command frames off the master and
/// answer each via `sim.respond` (QPIRI/QPIGS/QMOD/QPIWS across the driven cycles).
fn spawn_looping_voltronic_responder(
    master: PtyMaster,
    sim: VoltronicSim,
    exchanges: usize,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut f = master_file(&master);
        for _ in 0..exchanges {
            let Some(cmd) = read_command(&mut f) else {
                break;
            };
            if let Some(bytes) = sim.respond(&cmd) {
                let _ = f.write_all(&bytes);
                let _ = f.flush();
            }
        }
        thread::sleep(Duration::from_millis(50));
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

/// Inverter run config: identity `METEOR-001-inv`, fault every cycle (so QPIWS →
/// `active_diagnostic_state` is exercised each cycle). `dip_addresses` is unused by the inverter
/// leg; `interval_ms` is unused by `run_cycle_inverter` (only the outer `run_inverter` sleeps).
fn inverter_config() -> RunConfig {
    let mut id = heapless::String::new();
    id.push_str(INV_ID).unwrap();
    let mut prefix = heapless::String::new();
    prefix.push_str("tms").unwrap();
    RunConfig {
        device_id: id,
        prefix,
        dip_addresses: heapless::Vec::new(),
        interval_ms: 2000,
        fault_every: 1,
        max_backoff_ms: 30_000,
    }
}

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

async fn collect_publishes(eventloop: &mut rumqttc::EventLoop, want: usize) -> Vec<Publish> {
    let deadline = Duration::from_secs(8);
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

fn parse(p: &Publish) -> serde_json::Value {
    serde_json::from_slice(&p.payload).expect("valid json payload")
}

fn count_topic(msgs: &[Publish], topic: &str) -> usize {
    msgs.iter().filter(|p| p.topic == topic).count()
}

fn first_on<'a>(msgs: &'a [Publish], topic: &str) -> &'a Publish {
    msgs.iter()
        .find(|p| p.topic == topic)
        .unwrap_or_else(|| panic!("no publish on {topic}"))
}

/// THE proof (D9/D10/D10b): `run_cycle_inverter` over a real pty + broker publishes all four
/// inverter TMS topics AND the merged legacy inverter payload, under the inverter identity.
#[tokio::test]
#[file_serial(seplos_e2e)]
async fn voltronic_inverter_publishes_tms_and_legacy_through_the_real_pipeline() {
    // The sim reports an overload fault on QPIWS (index 15) — so active_diagnostic_state carries the
    // overload SPN 115, and (via the fault-merge cache) cycle 1's legacy payload carries the flag.
    let mut sim = VoltronicSim::new();
    sim.set_overload_fault();

    let (master, slave) = open_pty();
    // 2 cycles: cycle 0 = QPIRI + QPIGS + QMOD + QPIWS (4), cycle 1 = QPIGS + QMOD + QPIWS (3,
    // ratings held) = 7 exchanges total.
    let responder = spawn_looping_voltronic_responder(master, sim, 7);

    let (mut broker, port) = start_broker();

    let mut opts = MqttOptions::new("test-e2e-voltronic-sub", "127.0.0.1", port);
    opts.set_keep_alive(Duration::from_secs(5));
    let (client, mut eventloop) = AsyncClient::new(opts, 10);
    client
        .subscribe("#", QoS::AtLeastOnce)
        .await
        .expect("subscribe");
    wait_for_suback(&mut eventloop).await;

    // A GreengrassTap on the INVERTER identity/device_type — its own broker publisher handle, as
    // `bin-rpi::runtime` wires it per bus.
    let (_loc_tx, loc_rx) = tokio::sync::watch::channel(None);
    let gg_tap = GreengrassTap::new(broker.publisher(), INV_ID, "inverter", loc_rx);
    let sinks = SinkSet {
        taps: vec![HostTap::Greengrass(gg_tap)],
    };

    let mut platform = HostPlatform::build_port(
        broker.publisher(),
        sinks,
        "bus-inv".to_string(),
        &slave,
        // RS-232 for the Voltronic leg (full-duplex; direction control is a no-op).
        SerialKind::Rs232,
        SerialParams::default(),
        DirectionMode::default(),
        400,
        CyclePulse::default(),
        test_preflight_inputs(),
        None,
        Duration::from_secs(10),
    )
    .expect("build_port");

    let cfg = inverter_config();
    let vendor = Voltronic::new();
    let mut buf = [0u8; 256];
    let mut payload_buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    let mut health = Health::default();
    let mut ratings: Option<InverterRatings> = None;

    for cycle in 0..2u32 {
        let outcome = run_cycle_inverter(
            &mut platform,
            &cfg,
            &vendor,
            &mut buf,
            &mut payload_buf,
            cycle,
            &mut health,
            &mut ratings,
        )
        .await;
        assert!(
            outcome.any_success,
            "cycle {cycle}: the sim-encoded inverter frames must decode cleanly"
        );
    }

    // QPIRI succeeded on cycle 0 and was held for cycle 1.
    assert!(
        ratings.is_some(),
        "QPIRI ratings must have been polled and held"
    );

    // Cadence-conformant publish accounting (WS-S spec-conformance sweep, #265):
    //   cycle 0: AcMeasurementUpdate + DcMeasurementUpdate (Periodic 1 s) + PowerHardwareUpdate
    //            (Periodic 10 s, at-start) + ActiveDiagnosticState (At Start) + legacy = 5
    //   cycle 1: AcMeasurementUpdate + DcMeasurementUpdate + legacy = 3
    //            (PowerHardwareUpdate rate-limited to 10 s; ActiveDiagnosticState suppressed —
    //             the fault set + mode are unchanged, so On-Change does not re-fire)
    // => 8 publishes total.
    let msgs = collect_publishes(&mut eventloop, 8).await;

    let ac = format!("tms/{INV_ID}/ac_measurement_update");
    let dc = format!("tms/{INV_ID}/dc_measurement_update");
    let hw = format!("tms/{INV_ID}/power_hardware_update");
    let diag = format!("tms/{INV_ID}/active_diagnostic_state");
    let legacy = format!("telemetry/inverter/{INV_ID}");

    assert_eq!(
        count_topic(&msgs, &ac),
        2,
        "one AcMeasurementUpdate per cycle"
    );
    assert_eq!(
        count_topic(&msgs, &dc),
        2,
        "one DcMeasurementUpdate per cycle"
    );
    assert_eq!(
        count_topic(&msgs, &hw),
        1,
        "PowerHardwareUpdate is Periodic 10 s — only cycle 0 publishes within this 2-cycle window \
         (§4.9.10.2 rate limit)"
    );
    assert_eq!(
        count_topic(&msgs, &diag),
        1,
        "ActiveDiagnosticState is At Start + On Change — cycle 0 publishes the first snapshot; \
         cycle 1's fault set + mode are unchanged, so it is suppressed (B.12)"
    );
    assert_eq!(
        count_topic(&msgs, &legacy),
        2,
        "one merged legacy inverter payload per cycle"
    );

    // ---- TMS AcMeasurementUpdate: AC output (port 0) then grid (port 1), 230 V / 50 Hz ----------
    let ac_v = parse(first_on(&msgs, &ac));
    assert_eq!(ac_v["device_id"], INV_ID);
    let ext = ac_v["external_measurement"].as_array().expect("ext array");
    assert_eq!(ext.len(), 2, "AC output + grid input ports");
    assert_eq!(ext[0]["port_number"], 0, "port 0 = AC output");
    assert!((ext[0]["line"][0]["voltage"].as_f64().unwrap() - 230.0).abs() < 0.05);
    assert_eq!(ext[1]["port_number"], 1, "port 1 = grid input");
    // Grid current/power are unknown from QPIGS -> serialized as JSON null.
    assert!(ext[1]["line"][0]["amperage"].is_null());

    // ---- TMS DcMeasurementUpdate: PV (port 2) + battery (port 3), source-positive amperage -------
    let dc_v = parse(first_on(&msgs, &dc));
    let dext = dc_v["external_measurement"]
        .as_array()
        .expect("dc ext array");
    assert_eq!(dext.len(), 2);
    assert_eq!(dext[0]["port_number"], 2, "port 2 = PV");
    assert!((dext[0]["line"][0]["voltage"].as_f64().unwrap() - 380.0).abs() < 0.05);
    assert_eq!(dext[1]["port_number"], 3, "port 3 = battery DC");
    // 8 A charge, 0 A discharge -> source-positive amperage = -8 A.
    assert!((dext[1]["line"][0]["amperage"].as_f64().unwrap() - (-8.0)).abs() < 0.05);

    // ---- TMS PowerHardwareUpdate: only power_electronics.temperature -----------------------------
    let hw_v = parse(first_on(&msgs, &hw));
    assert!((hw_v["power_electronics"]["temperature"].as_f64().unwrap() - 27.0).abs() < 0.05);
    assert!(
        hw_v["energy_storage"].is_null(),
        "converter has no energy_storage state"
    );

    // ---- TMS ActiveDiagnosticState: overload SPN 115 --------------------------------------------
    let diag_v = parse(first_on(&msgs, &diag));
    let codes = diag_v["codes"].as_array().expect("codes array");
    assert_eq!(codes.len(), 1, "only the overload fault is set");
    assert_eq!(codes[0]["suspect_parameter"], 115, "overload -> SPN 115");
    assert_eq!(codes[0]["severity"], "SEV_7_MAJOR");

    // ---- Legacy inverter payload: merged flat JSON on telemetry/inverter/{inv-id} ---------------
    let legacy_msgs: Vec<serde_json::Value> = msgs
        .iter()
        .filter(|p| p.topic == legacy)
        .map(parse)
        .collect();
    for v in &legacy_msgs {
        assert_eq!(v["device_type"], "inverter");
        assert_eq!(v["device_id"], INV_ID);
        assert!(v["timestamp"].as_str().unwrap().ends_with('Z'));
        assert!((v["grid_voltage"].as_f64().unwrap() - 230.4).abs() < 1e-6);
        assert!((v["ac_output_voltage"].as_f64().unwrap() - 230.0).abs() < 1e-6);
        assert!((v["battery_voltage"].as_f64().unwrap() - 51.20).abs() < 1e-6);
        assert_eq!(v["device_mode"], "line_mode");
        assert_eq!(v["battery_type"], "lithium");
        assert_eq!(v["output_mode"], "single_phase");
        assert_eq!(v["load_on"], 1);
        assert_eq!(v["charging"], 1);
        assert_eq!(v["ac_charging"], 0);
    }
    // Fault-merge cache (D2): at least one legacy payload (cycle 1) carries the cached overload fault.
    assert!(
        legacy_msgs
            .iter()
            .any(|v| v.get("has_fault").and_then(|x| x.as_i64()) == Some(1)
                && v.get("overload_fault").and_then(|x| x.as_i64()) == Some(1)),
        "the inverter fault-merge cache must land the overload flag in a later legacy payload"
    );

    assert!(broker.is_running(), "the shared broker stays up");
    responder.join().ok();
}
