//! End-to-end `hotpath` profiling driver (Slice A — hotpath integration, Pi-only).
//!
//! Builds a REAL [`bin_rpi_platform::HostPlatform`] over a real pty-backed
//! `test_kit::SeplosSim` responder and a real embedded broker (`test_kit::start_broker`) — the
//! same production seam `test-e2e/tests/multi_port_e2e.rs`/`sigterm_drain_e2e.rs` drive, not a
//! hand-rolled `Platform` test double — then loops [`impl_app::run_cycle`] `HOTPATH_ITERS` times
//! (default [`DEFAULT_ITERS`]), timing each cycle via `hotpath::measure_block!` when the
//! `hotpath` feature is on. `HostPlatform::emit_reading`/`emit_fault`/`publish_rendered_tapped`
//! carry their own `#[cfg_attr(feature = "hotpath", hotpath::measure)]` annotations (see
//! `bin-rpi-platform/src/impl_host_platform.rs`), so a `--features hotpath` run reports both the
//! whole-cycle percentile and the platform's own publish-path breakdown underneath it.
//!
//! Run (Pi/host only — this crate never touches the no_std MCU build):
//! ```text
//! cargo run -p bin-rpi --example hotpath_e2e --features hotpath
//! HOTPATH_ITERS=20000 HOTPATH_OUTPUT_FORMAT=json \
//!     cargo run -p bin-rpi --example hotpath_e2e --features hotpath
//! ```
//! Without `--features hotpath` this still builds and runs as a plain, unmeasured loop —
//! `just lint`'s default `cargo clippy --workspace --all-targets` (feature-off) must stay clean,
//! so every `hotpath::` reference below is `#[cfg]`-gated out of that build entirely.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::thread;
use std::time::Duration;

use nix::fcntl::OFlag;
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt, PtyMaster};

use bin_rpi_platform::{CyclePulse, DirectionMode, HostPlatform, PreflightInputs, SinkSet};
use impl_app::{run_cycle, Health, RunConfig};
use lib_io::host::SerialKind;
use lib_io::transport::SerialParams;
use lib_mapping::sink::MAX_PAYLOAD;
use lib_parse::seplos::Seplos;
use test_kit::{start_broker, BatteryState, SeplosSim, SimReply};

/// Iterations run when `HOTPATH_ITERS` is unset.
const DEFAULT_ITERS: usize = 5000;

/// Open a pty pair; return the master handle and the slave device path (`/dev/pts/N`). Mirrors
/// `test-e2e/tests/multi_port_e2e.rs`'s helper of the same name (each pty-backed harness in this
/// workspace carries its own copy — see that file's doc comment for the established convention).
fn open_pty() -> (PtyMaster, String) {
    let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).expect("posix_openpt");
    grantpt(&master).expect("grantpt");
    unlockpt(&master).expect("unlockpt");
    // SAFETY: single-threaded bring-up; the path is used immediately.
    let slave = unsafe { ptsname(&master) }.expect("ptsname");
    (master, slave)
}

/// Borrow the master fd as a `File` without owning it (`PtyMaster` keeps ownership).
fn master_file(master: &PtyMaster) -> std::fs::File {
    use std::os::unix::io::FromRawFd;
    // SAFETY: the returned File is forgotten by the caller, so the fd is not closed.
    unsafe { std::fs::File::from_raw_fd(master.as_raw_fd()) }
}

/// Answer up to `iters` exchanges off the pty master, one command frame in -> one reply out per
/// iteration. Mirrors `test-e2e/tests/sigterm_drain_e2e.rs`'s `spawn_persistent_responder`
/// (unlike `multi_port_e2e.rs`'s one-shot `spawn_sim_responder`, this profiling driver needs many
/// exchanges, not one) — bounded by `iters` so the thread can never outlive the measured loop.
fn spawn_persistent_responder(
    master: PtyMaster,
    mut sim: SeplosSim,
    iters: usize,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut f = master_file(&master);
        for _ in 0..iters {
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
                break; // the pty closed (the driving loop already finished) or a spurious empty read
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

fn preflight_inputs() -> PreflightInputs {
    PreflightInputs {
        device_id_present: true,
        dip_addresses_present: true,
        cloud: None,
    }
}

/// One bus, one DIP, telemetry only (`fault_every: 0`) — isolates the
/// `DcMeasurementUpdate`/`PowerHardwareUpdate`/`StorageUpdate` publish path
/// (`HostPlatform::emit_reading`) that this driver is profiling.
fn run_config() -> RunConfig {
    let mut device_id = heapless::String::new();
    device_id
        .push_str("HOTPATH-E2E")
        .expect("literal fits Identity's heapless::String<32> capacity");
    let mut prefix = heapless::String::new();
    prefix
        .push_str("tms")
        .expect("literal fits RunConfig::prefix's heapless::String<16> capacity");
    let mut dip_addresses = heapless::Vec::new();
    dip_addresses
        .push(0)
        .expect("one entry fits RunConfig::dip_addresses's MAX_DIPS capacity");
    RunConfig {
        device_id,
        prefix,
        dip_addresses,
        // Unused: this driver calls `run_cycle` back-to-back (no `impl_app::run` outer sleep) so
        // the measured loop reflects the cycle's own cost, not an artificial interval.
        interval_ms: 0,
        fault_every: 0,
        max_backoff_ms: 30_000,
    }
}

/// One `run_cycle` activation. Isolated behind a plain fn so the call site is identical whether or
/// not it's wrapped in `hotpath::measure_block!` below — only [`run_measured_cycle`]'s two
/// `#[cfg]` variants differ.
async fn one_cycle(
    platform: &mut HostPlatform,
    cfg: &RunConfig,
    vendor: &Seplos,
    buf: &mut [u8],
    payload_buf: &mut [u8],
    cycle: u32,
    health: &mut Health,
) -> bool {
    run_cycle(platform, cfg, vendor, buf, payload_buf, cycle, health)
        .await
        .any_success
}

/// `hotpath`-measured cycle: wraps [`one_cycle`] in `hotpath::measure_block!("run_cycle", { … })`
/// so every activation's latency feeds the `#[hotpath::main]` percentile report on `main`'s exit.
#[cfg(feature = "hotpath")]
async fn run_measured_cycle(
    platform: &mut HostPlatform,
    cfg: &RunConfig,
    vendor: &Seplos,
    buf: &mut [u8],
    payload_buf: &mut [u8],
    cycle: u32,
    health: &mut Health,
) -> bool {
    hotpath::measure_block!("run_cycle", {
        one_cycle(platform, cfg, vendor, buf, payload_buf, cycle, health).await
    })
}

/// Plain fallback when the `hotpath` feature is off — the exact same call, unmeasured. This (not
/// `cfg!`/a runtime branch) is what keeps `hotpath::` entirely out of the feature-off build: the
/// `#[cfg(feature = "hotpath")]` sibling above is never even parsed into this build's AST.
#[cfg(not(feature = "hotpath"))]
async fn run_measured_cycle(
    platform: &mut HostPlatform,
    cfg: &RunConfig,
    vendor: &Seplos,
    buf: &mut [u8],
    payload_buf: &mut [u8],
    cycle: u32,
    health: &mut Health,
) -> bool {
    one_cycle(platform, cfg, vendor, buf, payload_buf, cycle, health).await
}

/// Build the real harness (pty responder + embedded broker + `HostPlatform`) and drive `iters`
/// cycles. Async body run from `main` via a plain `tokio::runtime::Runtime::block_on` (NOT
/// `#[tokio::main]`) — `#[hotpath::main]` requires a sync `fn main` (see this crate's Cargo.toml
/// comment / the task brief's verified hotpath facts), so `main` builds the runtime itself.
async fn drive(iters: usize) {
    let (master, slave) = open_pty();
    let sim = SeplosSim::new(BatteryState::sample_14_cell());
    let responder = spawn_persistent_responder(master, sim, iters);

    let (broker_guard, _port) = start_broker();
    let mut broker = broker_guard;

    let mut platform = HostPlatform::build_port(
        broker.publisher(),
        SinkSet::default(),
        "hotpath".to_string(),
        &slave,
        SerialKind::Rs232,
        SerialParams::default(),
        DirectionMode::default(),
        400,
        CyclePulse::default(),
        preflight_inputs(),
        None,
        Duration::from_secs(10),
    )
    .expect("build_port over the pty-backed slave");

    let cfg = run_config();
    let vendor = Seplos::new();
    let mut buf = [0u8; 256];
    let mut payload_buf = [0u8; MAX_PAYLOAD];
    let mut health = Health::default();
    let mut ok_count: usize = 0;

    for cycle in 0..iters {
        let ok = run_measured_cycle(
            &mut platform,
            &cfg,
            &vendor,
            &mut buf,
            &mut payload_buf,
            cycle as u32,
            &mut health,
        )
        .await;
        if ok {
            ok_count += 1;
        }
    }

    responder.join().ok();
    assert!(
        broker.is_running(),
        "the embedded broker must stay up for the entire measured run"
    );

    eprintln!(
        "hotpath_e2e: {ok_count}/{iters} cycles succeeded; health: tlm ok/err={}/{} pub ok/err={}/{}",
        health.telemetry_ok, health.telemetry_err, health.publish_ok, health.publish_err,
    );
}

// `hotpath-alloc` needs NO `allocator = …` argument and NO user `#[global_allocator]`: the
// `#[hotpath::main]` expansion installs hotpath's counting global allocator itself when the
// `hotpath-alloc` feature is active. Verified against hotpath v0.21.5 — a `--features
// hotpath,hotpath-alloc` run emits a populated `alloc-bytes` table (per-function exclusive heap
// bytes) with exactly the attribute below and nothing else. (`hotpath-cpu`'s sampler needs a
// samply worker that may not start in every environment; the CPU section then reports unavailable,
// which is non-fatal — timing + alloc are the metrics the bench gate consumes.)
#[cfg_attr(feature = "hotpath", hotpath::main(percentiles = [50, 95, 99]))]
fn main() {
    let iters: usize = std::env::var("HOTPATH_ITERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_ITERS);

    let rt = tokio::runtime::Runtime::new().expect("build a tokio runtime");
    rt.block_on(drive(iters));
}
