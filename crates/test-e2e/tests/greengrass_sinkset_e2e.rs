//! Legacy AWS Greengrass route through the REAL `SinkSet`/`HostTap::offer` dispatch (Scope D,
//! closes #52-#54, gap-closure #269) — the load-bearing proof.
//!
//! Every other Greengrass test (`legacy_greengrass_e2e.rs`) drives `GreengrassTap::offer` directly,
//! so none of them would have caught the shipped bug: `HostTap::offer`'s `Greengrass` arm was a
//! `log::trace` no-op that NEVER called `tap.offer()`, so `telemetry/#` never published in the real
//! binary despite a working renderer. This test closes that hole by exercising the entire
//! production seam end to end:
//!
//! * a real pty-backed [`SeplosSim`] responder (the same harness as `multi_port_e2e.rs`),
//! * the real `bin_rpi_platform::HostPlatform` built via `HostPlatform::build_port` with a real
//!   [`SinkSet`] that contains a real `HostTap::Greengrass(GreengrassTap)`,
//! * the real `impl_app::run_cycle` poll loop (telemetry every cycle, fault on the `fault_every`
//!   cadence), whose `emit_reading`/`emit_fault` call `SinkSet::offer` -> `HostTap::offer` ->
//!   `GreengrassTap::offer`.
//!
//! Because the dispatch goes through `HostTap::offer`, **re-stubbing that arm (removing the
//! `tap.offer(event)` call) makes this test fail** — the subscriber receives zero
//! `telemetry/battery/...` publishes and `collect_publishes` times out with fewer than the three
//! expected readings. That is exactly the regression guard #269 asks for.
//!
//! Fault-merge cache (D2): `run_cycle` emits `Reading` every cycle but `Fault` only every
//! `fault_every` cycles, and within a fault-due cycle the `Reading` is emitted BEFORE the `Fault`.
//! With `fault_every = 2` over cycles 0,1,2 the three published readings are therefore
//! `[no-fault, fault, fault]`: cycle 0's reading is rendered with an empty cache (its fault poll
//! populates the cache only afterward), and cycles 1 and 2 see the cached fault — proving the cache
//! persists across cycles (the `Arc<Mutex>`) and that flags stay ABSENT until the first successful
//! fault poll.
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

/// Read one command frame (bytes up to and including `\r`) off the pty master. Returns `None` on
/// EOF/error or a frame that never terminates within the bounded read budget.
fn read_command(f: &mut std::fs::File) -> Option<Vec<u8>> {
    let mut cmd = Vec::new();
    let mut byte = [0u8; 1];
    for _ in 0..64 {
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

/// Spawn a LOOPING `SeplosSim` responder: read a command frame off the master, answer it via
/// `sim.respond`, and repeat for up to `exchanges` frames (the exact number of command frames the
/// driver issues across the driven cycles — telemetry every cycle plus a fault on each fault-due
/// cycle). One `SeplosSim` serves both telemetry (`42`) and fault (`44`) commands, dispatching on
/// the parsed CID2.
fn spawn_looping_sim_responder(
    master: PtyMaster,
    mut sim: SeplosSim,
    exchanges: usize,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut f = master_file(&master);
        for _ in 0..exchanges {
            let Some(cmd) = read_command(&mut f) else {
                break;
            };
            match sim.respond(&cmd) {
                SimReply::Bytes(bytes) => {
                    let _ = f.write_all(bytes.as_slice());
                    let _ = f.flush();
                }
                SimReply::Silence => {}
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

/// One DIP (address 0), fault every 2nd cycle.
fn fault_every_two_config(device_id: &str) -> RunConfig {
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
        fault_every: 2,
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

/// Drain any further publishes that arrive within `dur` (used to prove NO extra message — e.g. a
/// `Fault` publishing one of its own — landed after the expected readings).
async fn drain_extra(eventloop: &mut rumqttc::EventLoop, dur: Duration) -> Vec<Publish> {
    let mut got = Vec::new();
    let _ = tokio::time::timeout(dur, async {
        loop {
            if let Ok(Event::Incoming(Incoming::Publish(p))) = eventloop.poll().await {
                got.push(p);
            }
        }
    })
    .await;
    got
}

fn parse(p: &Publish) -> serde_json::Value {
    serde_json::from_slice(&p.payload).expect("valid legacy json")
}

fn has_fault_keys(v: &serde_json::Value) -> bool {
    v.as_object()
        .map(|o| o.contains_key("high_voltage_protection"))
        .unwrap_or(false)
}

/// Assert the baseline-invariant fields that hold for EVERY merged reading in this run (no location
/// chain, `device_type` "battery", ISO-Z timestamp, baseline-disabled fields omitted).
fn assert_common_shape(v: &serde_json::Value) {
    let obj = v.as_object().expect("payload is a JSON object");
    assert_eq!(v["device_id"], "METEOR-001");
    assert_eq!(v["device_type"], "battery");
    let ts = v["timestamp"].as_str().expect("timestamp string");
    assert!(
        ts.ends_with('Z'),
        "ISO-8601 UTC timestamp must end 'Z': {ts}"
    );
    // No `[location]` chain wired -> latitude/longitude KEYS OMITTED entirely (baseline-exact; not
    // JSON null).
    assert!(
        !obj.contains_key("latitude"),
        "latitude key must be omitted"
    );
    assert!(
        !obj.contains_key("longitude"),
        "longitude key must be omitted"
    );
    assert_eq!(v["location_source"], "unknown");
    // Baseline-disabled fields never appear.
    assert!(
        !obj.contains_key("total_voltage"),
        "total_voltage must be omitted (config.yaml enabled: false)"
    );
    assert!(
        !obj.contains_key("rated_capacity"),
        "rated_capacity must be omitted (config.yaml enabled: false)"
    );
    assert!(!obj.contains_key("custom_number"));
}

/// The whole point (#269): drive `run_cycle` over a real `HostPlatform` whose `SinkSet` holds a
/// real `GreengrassTap`, and assert the tap actually publishes the merged flat legacy JSON — proving
/// the `HostTap::offer` dispatch is wired, and (via the fault-merge cache) that flags stay absent
/// until the first successful fault poll and are merged into subsequent readings.
#[tokio::test]
#[file_serial(seplos_e2e)]
async fn greengrass_publishes_through_the_real_sinkset_dispatch() {
    // Sim battery with two fault bits set — the fault poll must round-trip these into the cache.
    let mut state = BatteryState::sample_14_cell();
    state.faults.high_voltage_protection = true;
    state.faults.mosfet_fault = true;
    let mut sim = SeplosSim::new(state);
    sim.set_behavior(Behavior::Normal);

    let (master, slave) = open_pty();
    // 5 command frames total: cycle 0 (telemetry + fault) + cycle 1 (telemetry) + cycle 2
    // (telemetry + fault) with dips=[0], fault_every=2.
    let responder = spawn_looping_sim_responder(master, sim, 5);

    let (mut broker, port) = start_broker();

    // External subscriber on the legacy namespace ONLY — it will NOT see the primary `tms/#`
    // publishes, so every message it receives is a GreengrassTap publish.
    let mut opts = MqttOptions::new("test-e2e-greengrass-sinkset-sub", "127.0.0.1", port);
    opts.set_keep_alive(Duration::from_secs(5));
    let (client, mut eventloop) = AsyncClient::new(opts, 10);
    client
        .subscribe("telemetry/#", QoS::AtLeastOnce)
        .await
        .expect("subscribe");
    wait_for_suback(&mut eventloop).await;

    // The GreengrassTap's OWN broker publisher handle (a second `LocalBroker::publisher()`),
    // distinct from the platform's primary TMS publisher — exactly as `bin-rpi::runtime` wires it.
    let (_loc_tx, loc_rx) = tokio::sync::watch::channel(None);
    let gg_tap = GreengrassTap::new(broker.publisher(), "METEOR-001", "battery", loc_rx);
    let sinks = SinkSet {
        taps: vec![HostTap::Greengrass(gg_tap)],
    };

    let mut platform = HostPlatform::build_port(
        broker.publisher(),
        sinks,
        "bus-gg".to_string(),
        &slave,
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

    let cfg = fault_every_two_config("METEOR-001");
    let vendor = Seplos::new();
    let mut buf = [0u8; 256];
    let mut payload_buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    let mut health = Health::default();

    // Cycle 0 (fault-due): reading rendered with an EMPTY cache, then the fault poll populates it.
    // Cycle 1 (not fault-due): reading rendered with the cached fault (persists across cycles).
    // Cycle 2 (fault-due): reading rendered with the cached fault, then re-polls.
    for cycle in 0..3u32 {
        let outcome = run_cycle(
            &mut platform,
            &cfg,
            &vendor,
            &mut buf,
            &mut payload_buf,
            cycle,
            &mut health,
        )
        .await;
        assert!(
            outcome.any_success,
            "cycle {cycle}: the sim-encoded frame must decode cleanly"
        );
    }

    // Exactly three merged payloads reach the subscriber — one per reading (Fault events publish
    // nothing of their own).
    let msgs = collect_publishes(&mut eventloop, 3).await;
    assert_eq!(
        msgs.len(),
        3,
        "expected exactly three merged readings (one per cycle); got {} — a zero/short count means \
         HostTap::offer's Greengrass arm did not dispatch to GreengrassTap::offer (the #269 bug)",
        msgs.len()
    );
    let extra = drain_extra(&mut eventloop, Duration::from_millis(400)).await;
    assert!(
        extra.is_empty(),
        "no extra publish must arrive: a Fault event caches only, it never publishes its own \
         message (got {} extra)",
        extra.len()
    );

    // Every message is on the baseline topic, in arrival (== publish) order.
    for p in &msgs {
        assert_eq!(
            p.topic, "telemetry/battery/METEOR-001",
            "baseline topic namespace telemetry/{{device_type}}/{{device_id}}"
        );
    }
    let payloads: Vec<serde_json::Value> = msgs.iter().map(parse).collect();
    for v in &payloads {
        assert_common_shape(v);
        assert_eq!(v["dip_address"], 0);
    }

    // Fault-merge behavior: the FIRST reading (cycle 0, before its own fault poll took effect) has
    // NO fault-flag keys at all; the later readings carry the cached flags.
    assert!(
        !has_fault_keys(&payloads[0]),
        "reading #1 must have NO fault-flag keys (cache empty until the first fault poll succeeds)"
    );
    assert!(
        has_fault_keys(&payloads[1]),
        "reading #2 must carry the cached fault flags (cache persists across cycles)"
    );
    assert!(
        has_fault_keys(&payloads[2]),
        "reading #3 must carry the cached fault flags"
    );

    // The set fault bits round-trip through the sim -> driver -> canonical -> cache -> render as 1s,
    // with every other flag an explicit 0.
    for v in &payloads[1..] {
        assert_eq!(v["high_voltage_protection"], 1);
        assert_eq!(v["mosfet_fault"], 1);
        assert_eq!(v["low_voltage_protection"], 0);
        assert_eq!(v["short_circuit_protection"], 0);
        assert_eq!(v["charge_current_warning"], 0);
    }

    assert!(broker.is_running(), "the shared broker stays up");
    responder.join().ok();
}
