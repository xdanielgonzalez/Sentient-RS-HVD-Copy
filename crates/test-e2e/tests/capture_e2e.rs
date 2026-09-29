//! `[capture]` raw pre-parse wire-capture-to-disk, end to end (gap-closure #237, decision 0001's
//! open risk, Wave 4) — the load-bearing proof that `bin_rpi_platform::CaptureSerial` is actually
//! wired into the LIVE serial path via `bin-rpi::runtime::supervise`, not just unit-tested against
//! a hand-built `HostPlatform`.
//!
//! Spawns the **actual compiled `bin-rpi` binary** (same harness as `sigterm_drain_e2e.rs` — see
//! that file's doc comment for why this shells out to `cargo build` rather than
//! `env!("CARGO_BIN_EXE_bin-rpi")`) wired to a real pty-backed `SeplosSim` responder plus a
//! `[capture]` section, lets a few poll cycles complete, then asserts the resulting
//! `capture.jsonl` contains, for each captured exchange, the EXACT request/response bytes that
//! crossed the wire.
//!
//! The comparison is against an INDEPENDENT ground truth: the responder thread records every
//! exchange it actually read/wrote on the pty master as it happens, rather than this test
//! re-deriving the expected Seplos frame bytes from the protocol spec — re-deriving them would
//! only prove the test's own understanding of the protocol, not that `CaptureSerial` captured the
//! REAL wire bytes `bin-rpi`'s real driver exchanged.
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

use test_kit::{reserve_ephemeral_port, BatteryState, SeplosSim, SimReply};

/// Open a pty pair; return the master handle and the slave device path (`/dev/pts/N`). Mirrors
/// `sigterm_drain_e2e.rs`'s helper of the same name (each e2e file carries its own copy — see that
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

/// One observed request/response exchange (the exact bytes read/written on the pty master),
/// recorded by the responder thread as this test's independent ground truth.
type Exchange = (Vec<u8>, Vec<u8>);

/// Defensive cap on exchanges served — bin-rpi's poll loop runs forever, so this bounds the
/// responder thread even if this test's own deadlines somehow don't fire first.
const MAX_CYCLES: usize = 100;

/// Mirrors `sigterm_drain_e2e.rs`'s `spawn_persistent_responder`, plus recording every exchange it
/// actually put on the wire into `exchanges`.
fn spawn_recording_responder(
    master: PtyMaster,
    mut sim: SeplosSim,
    exchanges: Arc<Mutex<Vec<Exchange>>>,
) -> thread::JoinHandle<()> {
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
                exchanges
                    .lock()
                    .expect("exchanges mutex")
                    .push((cmd, bytes.as_slice().to_vec()));
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
/// `sigterm_drain_e2e.rs`'s doc comment for why this shells out rather than using
/// `CARGO_BIN_EXE_bin-rpi`.
fn ensure_bin_rpi_built() -> PathBuf {
    let root = workspace_root();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let manifest_path = root.join("Cargo.toml");

    let status = Command::new(&cargo)
        .args(["build", "-p", "bin-rpi", "--bin", "bin-rpi"])
        .arg("--manifest-path")
        .arg(&manifest_path)
        // Run from OUTSIDE the workspace tree — see `sigterm_drain_e2e.rs`'s identical comment for
        // why (Cargo's config-file discovery walks up from cwd, not `--manifest-path`).
        .current_dir(std::env::temp_dir())
        .status()
        .expect("spawn `cargo build -p bin-rpi`");
    assert!(
        status.success(),
        "`cargo build -p bin-rpi` failed; cannot run capture_e2e without a compiled bin-rpi binary"
    );

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
/// assertion earlier in the test never leaks a live process (mirrors `sigterm_drain_e2e.rs`'s
/// `BinRpiGuard`).
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
        .join(format!("capture-e2e-{}", std::process::id()))
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Decode a lowercase hex string back to bytes — test-only inverse of the producer's `to_hex` in
/// `lib_persist::impl_capture_writer`.
fn from_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

#[test]
#[file_serial(seplos_e2e)]
fn capture_records_the_exact_wire_bytes_of_each_exchange() {
    let bin_path = ensure_bin_rpi_built();

    // ---- Real pty-backed Seplos responder, recording every exchange it serves as ground truth.
    let (master, slave) = open_pty();
    let sim = SeplosSim::new(BatteryState::sample_14_cell());
    let exchanges: Arc<Mutex<Vec<Exchange>>> = Arc::new(Mutex::new(Vec::new()));
    let responder = spawn_recording_responder(master, sim, Arc::clone(&exchanges));

    // ---- Config: one legacy [serial] bus over the pty (bus name defaults to "default"),
    // fault_every = 0 so every cycle is a single telemetry write+read (one capture record per
    // cycle, no continuation-read fragmentation to reason about — see `impl_capture_serial.rs`'s
    // doc comment for that separate, inverter-only nuance), and a `[capture]` section pointed at a
    // scratch dir.
    let broker_port = reserve_ephemeral_port();
    let capture_dir = fresh_tmp_dir("capture");
    let cfg_dir = fresh_tmp_dir("cfg");
    let cfg_path = cfg_dir.join("sentient.toml");
    let jsonl_path = capture_dir.join("capture.jsonl");

    let cfg_toml = format!(
        r#"
[device]
id = "CAPTURE-TEST"

[serial]
kind = "rs232"
port = "{slave}"
baud = 19200
read_timeout_ms = 2000

[poll]
dip_addresses = [0]
interval_ms = 200
fault_every = 0

[broker]
host = "127.0.0.1"
port = {broker_port}

[publish]
prefix = "tms"

[capture]
dir = "{capture_dir}"
"#,
        slave = slave,
        broker_port = broker_port,
        capture_dir = capture_dir.display(),
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
    drop(child.stdout.take());

    let mut guard = BinRpiGuard(child);

    // ---- Wait for at least 3 captured exchanges on disk — proving the real serial exchange ->
    // CaptureSerial -> run_capture_writer -> disk pipeline runs end to end, not just that the
    // process started. `write_all`'s bytes land in the OS page cache immediately (see
    // `lib_persist::FsBackend::write_all`), so reading the file back from this separate process
    // needs no fsync/shutdown to have happened first.
    let startup_deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(contents) = std::fs::read_to_string(&jsonl_path) {
            if contents.lines().count() >= 3 {
                break;
            }
        }
        if Instant::now() > startup_deadline {
            let log = stderr_log.lock().expect("stderr log mutex").clone();
            panic!("bin-rpi did not write >=3 capture lines within 20s; stderr:\n{log}");
        }
        if let Ok(Some(status)) = guard.0.try_wait() {
            let log = stderr_log.lock().expect("stderr log mutex").clone();
            panic!(
                "bin-rpi exited early ({status:?}) before writing any capture lines; stderr:\n{log}"
            );
        }
        thread::sleep(Duration::from_millis(100));
    }

    let contents = std::fs::read_to_string(&jsonl_path).expect("read capture.jsonl");
    let lines: Vec<&str> = contents.lines().collect();
    assert!(
        lines.len() >= 3,
        "expected at least 3 captured exchanges, got {}",
        lines.len()
    );

    // The responder records an exchange BEFORE the driver's `read_until` can possibly complete and
    // reach `CaptureSerial`, so by the time N lines are on disk the responder must have observed
    // at least N exchanges.
    let recorded = exchanges.lock().expect("exchanges mutex").clone();
    assert!(
        recorded.len() >= lines.len(),
        "the responder must have observed at least as many exchanges ({}) as captured disk lines ({})",
        recorded.len(),
        lines.len()
    );

    for (i, line) in lines.iter().enumerate() {
        let v: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("capture line is not valid JSON: {e}\nline: {line}"));
        assert_eq!(
            v["bus"], "default",
            "a legacy [serial]-only config's bus name is always \"default\""
        );
        assert_eq!(
            v["valid"], true,
            "a successful transport-level read must record valid: true (line {i})"
        );
        let req_hex = v["request"].as_str().expect("request is a hex string");
        let resp_hex = v["response"].as_str().expect("response is a hex string");
        let req_bytes = from_hex(req_hex).expect("valid hex request");
        let resp_bytes = from_hex(resp_hex).expect("valid hex response");
        assert_eq!(
            req_bytes, recorded[i].0,
            "captured request bytes (line {i}) must match the exact bytes the sim received on the wire"
        );
        assert_eq!(
            resp_bytes, recorded[i].1,
            "captured response bytes (line {i}) must match the exact bytes the sim sent on the wire"
        );
        // Sanity: a real Seplos request frame always starts with '~' — proof this is genuinely
        // capturing Seplos wire frames, not empty/garbage bytes that happen to satisfy the byte
        // comparisons above.
        assert_eq!(
            req_bytes.first().copied(),
            Some(b'~'),
            "line {i}: a Seplos request frame must start with '~', got {req_hex}"
        );
        assert_eq!(
            req_bytes.last().copied(),
            Some(b'\r'),
            "line {i}: a Seplos request frame must end with '\\r', got {req_hex}"
        );
    }

    // ---- Tear down: kill bin-rpi (via the guard) before joining the stderr/responder threads,
    // which otherwise block on the pty/pipe staying open.
    drop(guard);
    stderr_thread.join().ok();
    responder.join().ok();
}
