//! SIGTERM graceful-shutdown drain, end to end (#247/#248, polish workstream, umbrella #237).
//!
//! Spawns the **actual compiled `bin-rpi` binary** (never re-implements
//! `bin-rpi::runtime::supervise`'s shutdown sequence) wired to a real pty-backed `SeplosSim`
//! responder plus `[disk]` JSON-lines logging, lets a few poll cycles complete, sends a real
//! `SIGTERM`, and asserts:
//!
//! 1. the process **exits with status 0** — the graceful path (`notify_stopping` ->
//!    `port_tasks.abort_all()` -> the shutdown broadcast -> a single 5 s drain timeout ->
//!    `Ok(())`, see `bin-rpi/src/runtime.rs`'s doc comment), not a signal-killed or nonzero exit —
//!    within a bounded window; and
//! 2. every JSONL line already written by the disk tap (`lib_persist::run_writer`, see
//!    `docs/modules/lib-persist.md`) is **complete, well-formed JSON** — the black-box proof that
//!    the drain-on-shutdown fsync path survives a REAL process teardown (a real SIGTERM racing
//!    real in-flight writes), not just an in-process `tokio::test`'s own teardown.
//!
//! **Why this doesn't use `env!("CARGO_BIN_EXE_bin-rpi")`:** that mechanism only resolves binary
//! targets belonging to the SAME package as the integration test (see `test-kit/tests/
//! tcp_sim_pacing.rs`, which works because `seplos-tcp-sim`'s `[[bin]]` is defined in `test-kit`
//! itself, the same crate as that test) — `bin-rpi` is a separate, `[[bin]]`-only package (no
//! `[lib]` target) that `test-e2e` does not (and, without Cargo's still-unstable
//! artifact-dependencies feature, cannot cleanly) depend on. Instead, [`ensure_bin_rpi_built`]
//! shells out to `cargo build -p bin-rpi --bin bin-rpi` (idempotent — a no-op rebuild-check if
//! already fresh) via the `CARGO` env var cargo itself provides at runtime, then resolves the
//! binary from the workspace's single shared `target/` directory. This also makes the test
//! self-sufficient under a scoped `cargo test -p test-e2e --test sigterm_drain_e2e` invocation,
//! not just a full `cargo test --workspace` run that happens to have already built every member.
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
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use serial_test::file_serial;

use test_kit::{reserve_ephemeral_port, BatteryState, SeplosSim, SimReply};

/// Open a pty pair; return the master handle and the slave device path (`/dev/pts/N`). Mirrors
/// `multi_port_e2e.rs`'s helper of the same name (each e2e file carries its own copy — see that
/// file's doc comment for the established per-file-duplication convention).
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

/// Drive the `SeplosSim` responder for as many exchanges as `bin-rpi`'s poll loop sends, unlike
/// `multi_port_e2e.rs`'s ONE-shot `spawn_sim_responder` (this test needs several completed cycles
/// before SIGTERM, not one). Reads one command frame per iteration and answers it, until a read
/// fails — which happens naturally once the pty's last slave-side fd closes (Linux ptys surface
/// that as `EIO` on the master once any already-queued bytes are drained), i.e. once `bin-rpi`
/// closes the port on exit — bounded by `MAX_CYCLES` as a defensive cap in case that assumption
/// doesn't hold on some kernel, so this thread can never block the test process forever.
const MAX_CYCLES: usize = 100;

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
                break; // the pty closed (bin-rpi exited) or a spurious empty read
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

/// Locate the workspace root two levels above this crate's manifest dir
/// (`<root>/crates/test-e2e`).
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("test-e2e is two directories under the workspace root")
        .to_path_buf()
}

/// Ensure a fresh `bin-rpi` binary exists, building it if necessary, and return its path. See
/// this module's doc comment for why this shells out rather than using `CARGO_BIN_EXE_bin-rpi`.
fn ensure_bin_rpi_built() -> PathBuf {
    let root = workspace_root();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let manifest_path = root.join("Cargo.toml");

    let status = Command::new(&cargo)
        .args(["build", "-p", "bin-rpi", "--bin", "bin-rpi"])
        .arg("--manifest-path")
        .arg(&manifest_path)
        // Run from OUTSIDE the workspace tree (`--manifest-path` above is already absolute, so
        // this doesn't change what gets built or where `target/` lands): Cargo's config-file
        // discovery walks up from the process's current directory, not from `--manifest-path`,
        // so a cwd inside the repo would pick up this workspace's `.cargo/config.toml` registry
        // source-replacement even though the manifest is named explicitly. Harmless in a normal
        // CI/dev environment where that registry mirror is reachable; needed in a sandbox where
        // it isn't (the same constraint documented across this umbrella story's other work
        // streams — see the integration handoff).
        .current_dir(std::env::temp_dir())
        .status()
        .expect("spawn `cargo build -p bin-rpi`");
    assert!(
        status.success(),
        "`cargo build -p bin-rpi` failed; cannot run the sigterm_drain_e2e black-box test without \
         a compiled bin-rpi binary"
    );

    // `cargo test`/`cargo build` without `--release` always use the debug profile, and this test
    // never overrides `CARGO_TARGET_DIR`/`--release`, so this is the one path to check.
    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("target"));
    let bin_path = target_dir.join("debug").join("bin-rpi");
    assert!(
        bin_path.is_file(),
        "expected a compiled bin-rpi at {bin_path:?} after a successful `cargo build -p bin-rpi`"
    );
    bin_path
}

/// Kills and reaps the wrapped `bin-rpi` child on drop if it's still running — so a failing
/// assertion earlier in the test never leaks a live process across test runs (mirrors
/// `test-kit/tests/tcp_sim_pacing.rs`'s `SimGuard`).
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
        .join(format!("sigterm-drain-e2e-{}", std::process::id()))
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[test]
#[file_serial(seplos_e2e)]
fn sigterm_drains_disk_log_and_exits_zero() {
    let bin_path = ensure_bin_rpi_built();

    // ---- Real pty-backed Seplos responder, answering every exchange bin-rpi's poll loop sends.
    let (master, slave) = open_pty();
    let sim = SeplosSim::new(BatteryState::sample_14_cell());
    let responder = spawn_persistent_responder(master, sim);

    // ---- Config: one bus over the pty, [disk] logging enabled, an ephemeral broker port so this
    // test never collides with another test's/instance's broker.
    let broker_port = reserve_ephemeral_port();
    let disk_dir = fresh_tmp_dir("disk");
    let cfg_dir = fresh_tmp_dir("cfg");
    let cfg_path = cfg_dir.join("sentient.toml");
    let jsonl_path = disk_dir.join("tms.jsonl");

    let cfg_toml = format!(
        r#"
[device]
id = "SIGTERM-TEST"

[serial]
kind = "rs232"
port = "{slave}"
baud = 19200
read_timeout_ms = 2000

[poll]
dip_addresses = [0]
interval_ms = 300
fault_every = 0

[broker]
host = "127.0.0.1"
port = {broker_port}

[publish]
prefix = "tms"

[disk]
dir = "{disk_dir}"
file_prefix = "tms"
fsync_every_lines = 1
queue_depth = 64
"#,
        slave = slave,
        broker_port = broker_port,
        disk_dir = disk_dir.display(),
    );
    std::fs::write(&cfg_path, cfg_toml).expect("write test config");

    // ---- Spawn the real bin-rpi binary, stderr piped and drained on a background thread (never
    // let a full pipe buffer block bin-rpi's own logging while we wait below).
    let mut child = Command::new(&bin_path)
        .env("SENTIENT_CONFIG", &cfg_path)
        .env_remove("CLOUD_MQTT_PASS")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bin-rpi");

    let stderr_log: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let stderr_log2 = Arc::clone(&stderr_log);
    let mut stderr_pipe = child.stderr.take().expect("piped stderr");
    let stderr_thread = thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr_pipe.read_to_string(&mut s);
        *stderr_log2.lock().expect("stderr log mutex") = s;
    });
    // stdout is piped but never read; drop it so bin-rpi's writes to it can't block on a full
    // pipe buffer either (env_logger writes to stderr, so stdout is expected to stay empty).
    drop(child.stdout.take());

    let mut guard = BinRpiGuard(child);

    // ---- Wait for at least one full poll cycle's worth of disk-logged lines (3 TMS topics per
    // successful exchange — DcMeasurementUpdate/PowerHardwareUpdate/StorageUpdate, see
    // bin_rpi_platform::HostPlatform::emit_reading) — proving the whole pipeline (serial exchange ->
    // decode -> TMS mapping -> broker publish -> disk tap) is genuinely running end to end, not
    // just that the process started.
    let startup_deadline = Instant::now() + Duration::from_secs(20);
    let lines_before_sigterm: usize = loop {
        if let Ok(contents) = std::fs::read_to_string(&jsonl_path) {
            let n = contents.lines().count();
            if n >= 3 {
                break n;
            }
        }
        if Instant::now() > startup_deadline {
            let log = stderr_log.lock().expect("stderr log mutex").clone();
            panic!(
                "bin-rpi did not write >=3 disk-logged telemetry lines within 20s; stderr:\n{log}"
            );
        }
        if let Ok(Some(status)) = guard.0.try_wait() {
            let log = stderr_log.lock().expect("stderr log mutex").clone();
            panic!(
                "bin-rpi exited early ({status:?}) before logging any telemetry; stderr:\n{log}"
            );
        }
        thread::sleep(Duration::from_millis(100));
    };

    // ---- Send a real SIGTERM and wait for the documented bounded-drain shutdown to complete.
    kill(Pid::from_raw(guard.0.id() as i32), Signal::SIGTERM).expect("send SIGTERM to bin-rpi");

    // `runtime::supervise`'s own drain budget is a single 5s timeout after the shutdown sequence
    // starts; this generously doubles it (plus signal-delivery/scheduling slack) rather than
    // pinning to exactly 5s, so this test asserts "bounded", not "exactly as fast as the
    // documented number down to the millisecond".
    let shutdown_deadline = Instant::now() + Duration::from_secs(12);
    let status = loop {
        match guard.0.try_wait().expect("try_wait") {
            Some(status) => break status,
            None => {
                if Instant::now() > shutdown_deadline {
                    let log = stderr_log.lock().expect("stderr log mutex").clone();
                    panic!(
                        "bin-rpi did not exit within 12s of SIGTERM (expected the ~5s bounded \
                         drain documented in bin-rpi/src/runtime.rs); stderr:\n{log}"
                    );
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
    };

    assert!(
        status.success(),
        "bin-rpi must exit 0 on a graceful SIGTERM shutdown, got {status:?}; stderr:\n{}",
        stderr_log.lock().expect("stderr log mutex")
    );

    stderr_thread.join().ok();
    responder.join().ok();

    // ---- The core assertion: every line already on disk is complete, well-formed JSON — no
    // partial/corrupted trailing line from a write racing the real SIGTERM — and the line count
    // never shrank (drain-on-shutdown must not lose already-queued records, #55).
    let contents = std::fs::read_to_string(&jsonl_path).expect("read jsonl after shutdown");
    let lines: Vec<&str> = contents.lines().collect();
    assert!(
        !lines.is_empty(),
        "expected at least one drained/fsynced JSONL line, got none"
    );
    assert!(
        lines.len() >= lines_before_sigterm,
        "line count must not shrink after graceful shutdown: before={lines_before_sigterm} after={}",
        lines.len()
    );

    let mut saw_tms_topic = false;
    for line in &lines {
        let v: serde_json::Value = serde_json::from_str(line).unwrap_or_else(|e| {
            panic!(
                "JSONL line is not valid JSON — drain/fsync must never leave a partial or \
                 corrupted line behind a real SIGTERM: {e}\nline: {line}"
            )
        });
        let topic = v
            .get("topic")
            .and_then(|t| t.as_str())
            .unwrap_or_else(|| panic!("envelope missing string `topic` field: {line}"));
        assert!(
            v.get("payload").is_some(),
            "envelope missing `payload` field: {line}"
        );
        if topic.starts_with("tms/SIGTERM-TEST/") {
            saw_tms_topic = true;
        }
    }
    assert!(
        saw_tms_topic,
        "expected at least one line under the tms/SIGTERM-TEST/ topic prefix, got: {lines:?}"
    );
}
