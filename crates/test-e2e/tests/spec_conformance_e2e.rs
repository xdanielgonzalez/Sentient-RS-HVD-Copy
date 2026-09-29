//! MIL-STD-3071 behavioral-conformance sweep, end to end (WS-S, umbrella #237; #263/#264/#265).
//!
//! Live-broker proof for the wave-5 spec-conformance work:
//! 1. [`embedded_broker_retains_device_info_for_late_subscribers`] — the embedded rumqttd broker
//!    replays a retained + QoS1 publish to a subscriber that connects AFTER it was published. This
//!    is the mechanism `bin_rpi::tasks::heartbeat` relies on for DeviceInfo's PublishLast /
//!    TRANSIENT_LOCAL profile (#265); a fast, no-`bin-rpi` probe so the retain capability is
//!    asserted independently of the heavier process tests below.
//! 2. [`stor_publishes_required_state_and_respects_10s_rate_limit`] — spawns the real `bin-rpi`
//!    binary against a pty Seplos sim and asserts (a) all four Required STOR state topics are
//!    published (#263), (b) `PowerHardwareUpdate`/`StorageUpdate` obey the Slow / Periodic 10 s
//!    rate limit while `DcMeasurementUpdate` publishes every ~0.5 s poll cycle (#265), and (c)
//!    `DeviceInfo` reaches a late-joining subscriber retained (#265).
//! 3. [`synthetic_mc_drives_active_microgrid_controller_state`] — a synthetic microgrid controller
//!    (test-published Heartbeat + DeviceInfo with role `ROLE_MICROGRID_CONTROLLER`) drives the
//!    MC-selection machine (#264): the device adopts it as master after the New-MC dwell, then
//!    reverts to `null` once the MC goes silent (Lost-Active-MC).
//!
//! Process-spawning pattern (pty responder, `ensure_bin_rpi_built`, `BinRpiGuard`) mirrors
//! `sigterm_drain_e2e.rs` — each e2e file carries its own copy of these helpers per this crate's
//! established per-file convention.
//!
//! **Timing**: both `bin-rpi` process tests (2 + 3) run the spawned binary with
//! `SENTIENT_TEST_FAST_TIMERS=1` (see `spawn_bin_rpi`), which — in this debug-build test binary
//! ONLY, never in a release build — switches `bin_rpi_platform::TimingProfile::resolve()` to the
//! compressed `TimingProfile::fast()` durations (New-MC/heartbeat-deadline 300 ms, Lost-Active-MC
//! 600 ms, No-MC 1 s, the 10 s rate limit -> 1 s). This asserts the SAME spec-mandated state
//! transitions and behaviors as the real MIL-STD-3071 timings (`TimingProfile::production`,
//! covered unit-test-side by `bin-rpi`'s `McSelector` tests, which construct via
//! `McSelector::new()` and are unaffected by this env var) — just compressed ~10x so this file
//! runs in seconds instead of minutes. See `TimingProfile`'s doc comment for the full
//! test-only-override contract.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use nix::fcntl::OFlag;
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt, PtyMaster};
use serial_test::file_serial;

use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, Publish, QoS};
use test_kit::{reserve_ephemeral_port, start_broker, BatteryState, SeplosSim, SimReply};

// ---- pty + process harness (mirrors sigterm_drain_e2e.rs) -------------------------------------

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

const MAX_CYCLES: usize = 400;

/// Answer every Seplos exchange `bin-rpi` sends until the pty closes (bin-rpi exits) or the
/// defensive `MAX_CYCLES` cap is hit.
fn spawn_persistent_responder(master: PtyMaster, mut sim: SeplosSim) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut f = master_file(&master);
        for _ in 0..MAX_CYCLES {
            let mut cmd = Vec::new();
            let mut byte = [0u8; 1];
            let mut got_any = false;
            for _ in 0..64 {
                match f.read(&mut byte) {
                    Ok(1) => {
                        got_any = true;
                        cmd.push(byte[0]);
                        if byte[0] == b'\r' {
                            break;
                        }
                    }
                    _ => break,
                }
            }
            if !got_any {
                break;
            }
            if cmd.is_empty() {
                continue;
            }
            if let SimReply::Bytes(bytes) = sim.respond(&cmd) {
                let _ = f.write_all(bytes.as_slice());
                let _ = f.flush();
            }
        }
        std::mem::forget(f);
        drop(master);
    })
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("test-e2e is two directories under the workspace root")
        .to_path_buf()
}

/// Build (idempotently) and locate the `bin-rpi` binary — see `sigterm_drain_e2e.rs` for why this
/// shells out rather than using `CARGO_BIN_EXE_bin-rpi`.
fn ensure_bin_rpi_built() -> PathBuf {
    let root = workspace_root();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let manifest_path = root.join("Cargo.toml");
    let status = Command::new(&cargo)
        .args(["build", "-p", "bin-rpi", "--bin", "bin-rpi"])
        .arg("--manifest-path")
        .arg(&manifest_path)
        .current_dir(std::env::temp_dir())
        .status()
        .expect("spawn `cargo build -p bin-rpi`");
    assert!(status.success(), "`cargo build -p bin-rpi` failed");
    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("target"));
    let bin_path = target_dir.join("debug").join("bin-rpi");
    assert!(
        bin_path.is_file(),
        "expected a compiled bin-rpi at {bin_path:?}"
    );
    bin_path
}

struct BinRpiGuard(Child);

impl Drop for BinRpiGuard {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn fresh_tmp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("spec-conf-e2e-{}", std::process::id()))
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

// ---- async MQTT helpers -----------------------------------------------------------------------

/// Drive `eventloop` until a `SubAck` arrives (tolerating the connection races of a broker that may
/// still be starting), bounded by `deadline`.
async fn wait_for_suback(eventloop: &mut rumqttc::EventLoop, deadline: Duration) {
    tokio::time::timeout(deadline, async {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Incoming::SubAck(_))) => return,
                Ok(_) => {}
                Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
            }
        }
    })
    .await
    .expect("broker acknowledged the subscription within the deadline");
}

/// Collect every inbound `Publish` for `dur`.
async fn collect_for(eventloop: &mut rumqttc::EventLoop, dur: Duration) -> Vec<Publish> {
    let mut got = Vec::new();
    let _ = tokio::time::timeout(dur, async {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Incoming::Publish(p))) => got.push(p),
                Ok(_) => {}
                Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        }
    })
    .await;
    got
}

/// Poll until a `Publish` satisfying `pred` arrives or `deadline` elapses.
async fn wait_for_publish(
    eventloop: &mut rumqttc::EventLoop,
    deadline: Duration,
    mut pred: impl FnMut(&Publish) -> bool,
) -> Option<Publish> {
    tokio::time::timeout(deadline, async {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Incoming::Publish(p))) => {
                    if pred(&p) {
                        return p;
                    }
                }
                Ok(_) => {}
                Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        }
    })
    .await
    .ok()
}

fn count_topic(msgs: &[Publish], topic: &str) -> usize {
    msgs.iter().filter(|p| p.topic == topic).count()
}

fn json_of(msgs: &[Publish], topic: &str) -> Option<serde_json::Value> {
    msgs.iter()
        .find(|p| p.topic == topic)
        .map(|p| serde_json::from_slice(&p.payload).expect("valid JSON payload"))
}

// ---- 1. Retained-DeviceInfo mechanism probe (fast, no bin-rpi) --------------------------------

#[tokio::test]
#[file_serial(seplos_e2e)]
async fn embedded_broker_retains_device_info_for_late_subscribers() {
    let (broker, port) = start_broker();

    // A publisher publishes DeviceInfo retained + QoS1 *before* any subscriber exists — exactly
    // what `bin_rpi::tasks::heartbeat` does at startup.
    let mut pub_opts = MqttOptions::new("spec-conf-retain-pub", "127.0.0.1", port);
    pub_opts.set_keep_alive(Duration::from_secs(5));
    let (publisher, mut pub_loop) = AsyncClient::new(pub_opts, 10);
    publisher
        .publish(
            "tms/RETAIN-TEST/device_info",
            QoS::AtLeastOnce,
            true, // retain
            br#"{"device_id":"RETAIN-TEST","role":"ROLE_STORAGE"}"#.to_vec(),
        )
        .await
        .expect("publish retained DeviceInfo");
    // Pump the publisher's eventloop briefly so the packet actually flushes to the broker.
    let _ = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if pub_loop.poll().await.is_err() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    })
    .await;

    // A LATE subscriber connects and must immediately receive the retained DeviceInfo.
    let mut sub_opts = MqttOptions::new("spec-conf-retain-sub", "127.0.0.1", port);
    sub_opts.set_keep_alive(Duration::from_secs(5));
    let (subscriber, mut sub_loop) = AsyncClient::new(sub_opts, 10);
    subscriber
        .subscribe("tms/RETAIN-TEST/device_info", QoS::AtLeastOnce)
        .await
        .expect("subscribe");

    let msg = wait_for_publish(&mut sub_loop, Duration::from_secs(5), |p| {
        p.topic == "tms/RETAIN-TEST/device_info"
    })
    .await
    .expect(
        "a late subscriber must receive the retained DeviceInfo — the embedded broker replays \
         retained messages (DeviceInfo PublishLast profile, #265)",
    );
    assert!(
        msg.retain,
        "the replayed DeviceInfo must carry the retain flag (retained-on-subscribe delivery)"
    );

    assert!(broker.is_running());
}

// ---- shared bin-rpi spawn for tests 2 + 3 -----------------------------------------------------

const DEVICE_ID: &str = "SPEC-CONF";

/// Spawn `bin-rpi` (STOR/Seplos) over a pty sim on `broker_port`, draining stderr on a thread.
/// Returns the process guard, the responder join handle, and the captured-stderr handle.
fn spawn_bin_rpi(bin: &Path, slave: &str, broker_port: u16) -> (BinRpiGuard, Arc<Mutex<String>>) {
    let cfg_dir = fresh_tmp_dir("cfg");
    let cfg_path = cfg_dir.join("sentient.toml");
    let cfg_toml = format!(
        r#"
[device]
id = "{DEVICE_ID}"

[serial]
kind = "rs232"
port = "{slave}"
baud = 19200
read_timeout_ms = 2000

[poll]
dip_addresses = [0]
interval_ms = 500
fault_every = 0

[broker]
host = "127.0.0.1"
port = {broker_port}

[publish]
prefix = "tms"
"#
    );
    std::fs::write(&cfg_path, cfg_toml).expect("write test config");

    let mut child = Command::new(bin)
        .env("SENTIENT_CONFIG", &cfg_path)
        // TEST-ONLY: compresses the McSelector/rate-limit timers ~10x (see this file's header
        // doc comment + `bin_rpi_platform::TimingProfile`). `ensure_bin_rpi_built` builds a debug
        // binary, so `TimingProfile::resolve()`'s `#[cfg(debug_assertions)]` gate is open here;
        // a release `bin-rpi` ignores this var entirely and always runs production timings.
        .env("SENTIENT_TEST_FAST_TIMERS", "1")
        .env_remove("CLOUD_MQTT_PASS")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bin-rpi");

    let stderr_log: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let stderr_log2 = Arc::clone(&stderr_log);
    let mut stderr_pipe = child.stderr.take().expect("piped stderr");
    thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr_pipe.read_to_string(&mut s);
        *stderr_log2.lock().expect("stderr log mutex") = s;
    });

    (BinRpiGuard(child), stderr_log)
}

// ---- 2. Required STOR state topics + 10 s rate limit ------------------------------------------

#[tokio::test]
#[file_serial(seplos_e2e)]
async fn stor_publishes_required_state_and_respects_10s_rate_limit() {
    let bin = ensure_bin_rpi_built();
    let (master, slave) = open_pty();
    let responder =
        spawn_persistent_responder(master, SeplosSim::new(BatteryState::sample_14_cell()));
    let broker_port = reserve_ephemeral_port();
    let (_guard, stderr_log) = spawn_bin_rpi(&bin, &slave, broker_port);

    // Subscribe to everything under this device from the start.
    let mut opts = MqttOptions::new("spec-conf-state-sub", "127.0.0.1", broker_port);
    opts.set_keep_alive(Duration::from_secs(5));
    let (client, mut eventloop) = AsyncClient::new(opts, 200);
    client
        .subscribe(format!("tms/{DEVICE_ID}/#"), QoS::AtLeastOnce)
        .await
        .expect("subscribe");
    wait_for_suback(&mut eventloop, Duration::from_secs(20)).await;

    // Collect a window comfortably longer than the FAST-profile rate_limit_period (1 s — see this
    // file's header doc comment on `SENTIENT_TEST_FAST_TIMERS`/`TimingProfile::fast`) so
    // PowerHardwareUpdate/StorageUpdate get at least one extra publish at that boundary while
    // DcMeasurementUpdate accrues every ~0.5 s cycle (unaffected by the profile — that cadence
    // comes from this test's own `[poll] interval_ms = 500`). Bounded below 2x the rate limit so
    // the rate-limited topics stay capped at 2 publishes (mirrors the original production-timing
    // test's ~1.3x-of-10s-rate-limit window, scaled to the compressed profile with extra relative
    // margin for real (non-scaling) process/MQTT startup latency).
    let msgs = collect_for(&mut eventloop, Duration::from_millis(1800)).await;

    let topic = |t: &str| format!("tms/{DEVICE_ID}/{t}");

    // (a) All four Required STOR state topics were published.
    for t in [
        "energy_start_stop_state",
        "power_port_state",
        "dc_load_sharing_state",
        "active_microgrid_controller_state",
    ] {
        assert!(
            msgs.iter().any(|p| p.topic == topic(t)),
            "expected at least one publish on tms/{DEVICE_ID}/{t}; stderr:\n{}",
            stderr_log.lock().unwrap()
        );
    }

    // Honest content spot-checks.
    let ess = json_of(&msgs, &topic("energy_start_stop_state")).expect("EnergyStartStopState");
    assert_eq!(ess["present_level"], "ESSL_OPERATIONAL");
    assert_eq!(ess["present_level_reason"], "PSR_STARTUP");
    let dls = json_of(&msgs, &topic("dc_load_sharing_state")).expect("DcLoadSharingState");
    assert_eq!(dls["config"], "CONFIG_UNKNOWN");
    assert_eq!(dls["parameters"].as_array().unwrap().len(), 0);
    let amc = json_of(&msgs, &topic("active_microgrid_controller_state")).expect("AMCState");
    assert!(
        amc["master_id"].is_null(),
        "with no MC on the bus, AMCState.master_id must be null; got {amc}"
    );

    // (b) Cadence: DcMeasurementUpdate every cycle vs the two rate_limit_period-gated topics.
    let dc = count_topic(&msgs, &topic("dc_measurement_update"));
    let ph = count_topic(&msgs, &topic("power_hardware_update"));
    let su = count_topic(&msgs, &topic("storage_update"));
    assert!(
        dc >= 3,
        "DcMeasurementUpdate (Periodic 1 s / unconditional every ~0.5 s poll cycle) should \
         publish at least 3 times in a ~1.8 s window; got {dc}"
    );
    assert!(
        (1..=2).contains(&ph),
        "PowerHardwareUpdate (rate-limited to the FAST-profile ~1 s rate_limit_period) should \
         publish once (at start) or twice (~1 s later) in a ~1.8 s window, never once-per-cycle; \
         got {ph}"
    );
    assert!(
        (1..=2).contains(&su),
        "StorageUpdate (rate-limited to the FAST-profile ~1 s rate_limit_period) should publish \
         1-2 times in ~1.8 s; got {su}"
    );
    assert!(
        dc >= ph,
        "DcMeasurementUpdate ({dc}) must be at least as frequent as the rate-limited \
         PowerHardwareUpdate ({ph}) — DcMeasurementUpdate publishes unconditionally on every \
         poll cycle while PowerHardwareUpdate is gated to at most once per rate_limit_period, so \
         dc can never trail ph; a violation would mean the gate stopped applying to \
         PowerHardwareUpdate instead of DcMeasurementUpdate correctly bypassing it"
    );

    // (c) DeviceInfo reaches a late-joining subscriber retained (PublishLast).
    let mut late_opts = MqttOptions::new("spec-conf-late-sub", "127.0.0.1", broker_port);
    late_opts.set_keep_alive(Duration::from_secs(5));
    let (late, mut late_loop) = AsyncClient::new(late_opts, 20);
    late.subscribe(topic("device_info"), QoS::AtLeastOnce)
        .await
        .expect("late subscribe device_info");
    let di = wait_for_publish(&mut late_loop, Duration::from_secs(8), |p| {
        p.topic == topic("device_info")
    })
    .await
    .expect("a late subscriber must receive the retained DeviceInfo (PublishLast, #265)");
    assert!(
        di.retain,
        "DeviceInfo must be delivered retained to the late joiner"
    );

    // Kill `bin-rpi` BEFORE joining the responder thread: the responder's reads block on the pty
    // master as long as `bin-rpi` still holds the slave end open (it polls forever), so joining
    // first would wait on however many exchange cycles `bin-rpi` happens to run before this
    // function would otherwise have dropped `_guard` at scope end (potentially very slow — the
    // very hang this test's timing rework is meant to avoid). Dropping `_guard` here closes the
    // slave, so the responder's next read sees EOF and its loop exits immediately.
    drop(_guard);
    responder.join().ok();
}

// ---- 3. MC-selection: synthetic MC drives ActiveMicrogridControllerState ----------------------

#[tokio::test]
#[file_serial(seplos_e2e)]
async fn synthetic_mc_drives_active_microgrid_controller_state() {
    let bin = ensure_bin_rpi_built();
    let (master, slave) = open_pty();
    let responder =
        spawn_persistent_responder(master, SeplosSim::new(BatteryState::sample_14_cell()));
    let broker_port = reserve_ephemeral_port();
    let (_guard, _stderr) = spawn_bin_rpi(&bin, &slave, broker_port);

    let amc_topic = format!("tms/{DEVICE_ID}/active_microgrid_controller_state");

    let mut opts = MqttOptions::new("spec-conf-mc-sub", "127.0.0.1", broker_port);
    opts.set_keep_alive(Duration::from_secs(5));
    let (client, mut eventloop) = AsyncClient::new(opts, 200);
    client
        .subscribe(amc_topic.clone(), QoS::AtLeastOnce)
        .await
        .expect("subscribe AMCState");
    wait_for_suback(&mut eventloop, Duration::from_secs(20)).await;

    // Initially (no MC on the bus) AMCState reports no master. Not itself gated by
    // `TimingProfile` (process/preflight startup only), but bounded well below its old 8 s
    // deadline now that the rest of this test runs in a few seconds too.
    let first = wait_for_publish(&mut eventloop, Duration::from_secs(3), |p| {
        p.topic == amc_topic
    })
    .await
    .expect("initial AMCState publish");
    let first_v: serde_json::Value = serde_json::from_slice(&first.payload).unwrap();
    assert!(
        first_v["master_id"].is_null(),
        "before any MC appears, master_id must be null; got {first_v}"
    );

    // Publish a synthetic microgrid controller: a MC-role DeviceInfo (retained so the selector
    // sees it regardless of ordering) plus Heartbeats well inside the FAST-profile heartbeat
    // deadline (300 ms — see this file's header doc comment) so consecutive beats are treated as
    // one continuous streak instead of each starting a fresh New-MC dwell.
    const MC_ID: &str = "MC-1";
    const BEAT_INTERVAL: Duration = Duration::from_millis(100);
    client
        .publish(
            format!("tms/{MC_ID}/device_info"),
            QoS::AtLeastOnce,
            true,
            format!(r#"{{"device_id":"{MC_ID}","role":"ROLE_MICROGRID_CONTROLLER"}}"#).into_bytes(),
        )
        .await
        .expect("publish MC device_info");

    // Beat while draining inbound AMCState — AMCState is published On Change (once, null->MC-1), so
    // we must observe the transition INLINE with the beating, not after it. Succeed as soon as
    // master_id == MC-1 is seen (which happens after the FAST-profile New-MC dwell, 300 ms).
    // 2 s bounds the wait with generous margin over that 300 ms dwell (MQTT round trips + the
    // 50 ms eval tick), while being ~5x faster than the old production-timing 10 s deadline.
    let mut selected = false;
    let select_deadline = Instant::now() + Duration::from_secs(2);
    let mut last_beat = Instant::now() - (BEAT_INTERVAL * 2);
    while Instant::now() < select_deadline {
        if last_beat.elapsed() >= BEAT_INTERVAL {
            client
                .publish(
                    format!("tms/{MC_ID}/heartbeat"),
                    QoS::AtMostOnce,
                    false,
                    br#"{"device_id":"MC-1","sequence_number":0}"#.to_vec(),
                )
                .await
                .ok();
            last_beat = Instant::now();
        }
        if let Ok(Ok(Event::Incoming(Incoming::Publish(p)))) =
            tokio::time::timeout(Duration::from_millis(30), eventloop.poll()).await
        {
            if p.topic == amc_topic
                && serde_json::from_slice::<serde_json::Value>(&p.payload)
                    .map(|v| v["master_id"] == serde_json::json!(MC_ID))
                    .unwrap_or(false)
            {
                selected = true;
                break;
            }
        }
    }
    assert!(
        selected,
        "after the New-MC dwell (FAST-profile 300 ms of MC Heartbeats), AMCState.master_id must \
         become MC-1"
    );

    // Stop beating. After Lost-Active-MC (FAST-profile 600 ms), the device must revert to no
    // master. 2.5 s bounds the wait with generous margin (>4x) over that trigger point.
    let reverted = wait_for_publish(&mut eventloop, Duration::from_millis(2500), |p| {
        if p.topic != amc_topic {
            return false;
        }
        serde_json::from_slice::<serde_json::Value>(&p.payload)
            .map(|v| v["master_id"].is_null())
            .unwrap_or(false)
    })
    .await
    .expect("after the MC goes silent (Lost-Active-MC), AMCState.master_id must revert to null");
    let reverted_v: serde_json::Value = serde_json::from_slice(&reverted.payload).unwrap();
    assert!(reverted_v["master_id"].is_null());

    // Kill `bin-rpi` BEFORE joining the responder thread — see the matching comment in
    // `stor_publishes_required_state_and_respects_10s_rate_limit` for why this order matters.
    drop(_guard);
    responder.join().ok();
}
