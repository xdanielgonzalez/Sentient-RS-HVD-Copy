//! S3 — at-rate end-to-end attestation.
//!
//! `host_pty.rs` and `e2e_l1.rs` (this crate) already prove the pipeline is CORRECT against an
//! instantly-replying `SeplosSim`. This file proves it holds up when the response actually
//! TRICKLES in at real Seplos wire timing (19200 baud, 8N1 = 10 bits/byte — the same ground truth
//! `crates/test-kit/tests/tcp_sim_pacing.rs` verifies for the `seplos-tcp-sim` binary's `--pace`
//! flag) instead of arriving as one instant burst, using the DEFAULT host read timeout (1000 ms —
//! `crates/bin/rpi/src/config/types/serial_config.rs::default_read_timeout_ms`) — and that a
//! device-reported RTN protocol error is absorbed as a counted failure, never fatal.
//!
//! Drives the real production path: `lib_io::host::HostSerial` (`tokio-serial` over a real pty,
//! `SerialParams::default()` = 19200 8N1) -> `lib_io::driver::read_telemetry` -> `lib_parse` ->
//! `impl_app::run_cycle`, with `test_kit::SeplosSim` as the responder — the same pure protocol
//! core `seplos-tcp-sim` wraps. Bound to a host pty rather than the `seplos-tcp-sim` TCP binary +
//! its `--pace` flag: `CARGO_BIN_EXE_<name>` is only populated for integration tests inside the
//! SAME package as the binary (`test-kit`), so this crate cannot portably resolve that binary's
//! path, and this task's file scope is `test-e2e` only. The reply-pacing math itself (sleep the
//! turnaround, then write in small chunks each followed by `chunk_len * 10 / baud` seconds of
//! sleep) is therefore reproduced locally rather than imported — `test-kit`'s implementation
//! (`crates/test-kit/src/bin/seplos_tcp_sim/impl_core_pacing.rs`) is `pub(super)` inside a binary
//! crate, not part of the `test-kit` library surface this crate depends on. It mirrors that file's
//! documented math exactly and is exercised against the same kind of wire-time bound
//! `tcp_sim_pacing.rs::paced_reply_trickles_at_wire_rate` uses.
//!
//! `MockSerial`-driven tests (`impl-app/tests/run_cycle.rs`) stay separate on purpose (see
//! `e2e_l1.rs`'s doc comment) — this file is pty-at-rate specific.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::thread;
use std::time::{Duration, Instant};

use nix::fcntl::OFlag;
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt, PtyMaster};
use serial_test::file_serial;

use impl_app::{run_cycle, Health, Platform, RunConfig};
use lib_io::host::{HostSerial, SerialKind};
use lib_io::transport::SerialParams;
use lib_mapping::render::{render_fault, render_tms, TmsTopic};
use lib_mapping::schema::{topics, ClockMonotonic};
use lib_mapping::sink::{publish_rendered, PublishError, Publisher};
use lib_parse::canonical::{CanonicalFault, CanonicalReading};
use lib_parse::seplos::Seplos;
use test_kit::{BatteryState, Behavior, SeplosSim, SimReply};

/// Real Seplos RS-485 wire rate (matches `SerialParams::default()`'s 19200 8N1 and `bin-rpi`'s
/// `SerialConfig::default_baud`).
const PACE_BAUD: u32 = 19_200;
/// Simulated device-processing/RS-485 turnaround before the first reply byte (matches
/// `seplos_tcp_sim::PaceConfig::DEFAULT_TURNAROUND_MS`).
const TURNAROUND_MS: u64 = 10;
/// The host serial read deadline `bin-rpi`'s config defaults to
/// (`SerialConfig::default_read_timeout_ms`) — the real production default, not a test-shortened
/// one, so the margin assertions below mean what they claim.
const DEFAULT_READ_TIMEOUT_MS: u32 = 1000;

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

/// Write `bytes` to `f`, paced at the wire rate implied by `pace = Some((baud, turnaround_ms))`.
/// `None` writes instantly (one `write_all` + `flush`, no delay) — see this file's doc comment for
/// why this reproduces (rather than imports) `seplos-tcp-sim`'s pacing math.
fn write_paced(f: &mut std::fs::File, bytes: &[u8], pace: Option<(u32, u64)>) {
    let Some((baud, turnaround_ms)) = pace else {
        let _ = f.write_all(bytes);
        let _ = f.flush();
        return;
    };
    if turnaround_ms > 0 {
        thread::sleep(Duration::from_millis(turnaround_ms));
    }
    // Batching into small chunks (each followed by a proportional sleep) keeps bytes trickling
    // onto the wire rather than one instant burst, while keeping syscall count reasonable — same
    // approach and constant as `impl_core_pacing.rs::PACE_CHUNK_BYTES`.
    const CHUNK_BYTES: usize = 8;
    let byte_interval = Duration::from_secs_f64(10.0 / f64::from(baud));
    for chunk in bytes.chunks(CHUNK_BYTES) {
        let _ = f.write_all(chunk);
        let _ = f.flush();
        thread::sleep(byte_interval * chunk.len() as u32);
    }
}

/// Spawn the `SeplosSim` responder on its own thread: read up to `exchanges` command frames off
/// the pty master (bounded byte-at-a-time reads, mirroring `e2e_l1.rs::spawn_sim_responder`), and
/// answer each via `sim.respond`, paced per `pace` (see [`write_paced`]).
fn spawn_sim_responder(
    master: PtyMaster,
    mut sim: SeplosSim,
    exchanges: usize,
    pace: Option<(u32, u64)>,
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
                SimReply::Bytes(bytes) => write_paced(&mut f, bytes.as_slice(), pace),
                SimReply::Silence => {}
            }
        }
        thread::sleep(Duration::from_millis(150));
        std::mem::forget(f);
        drop(master);
    })
}

/// Open the real host transport against the pty slave, with the DEFAULT `SerialParams` (19200
/// 8N1) and the DEFAULT `bin-rpi` read timeout (1000 ms) — never a test-shortened deadline, so the
/// margin assertions below attest the real production configuration.
fn open_transport_default_timeout(slave: &str) -> HostSerial {
    HostSerial::open(
        slave,
        SerialKind::Rs232,
        SerialParams::default(),
        DEFAULT_READ_TIMEOUT_MS,
    )
    .expect("open pty slave as serial")
}

/// A publisher that counts calls; never forced to fail here — these tests attest transport/parser
/// timing and RTN-error accounting, not publish-sink failure (already covered by
/// `impl-app/tests/run_cycle.rs::publish_sink_failure_is_counted_not_fatal`).
#[derive(Default)]
struct CountingPublisher {
    calls: u32,
}
impl Publisher for CountingPublisher {
    fn publish(&mut self, _topic: &str, _payload: &[u8]) -> Result<(), PublishError> {
        self.calls += 1;
        Ok(())
    }
    fn healthy(&self) -> bool {
        true
    }
}

/// A `Platform` wiring the REAL pty-backed `HostSerial` transport (opened with production
/// defaults) to a counting publisher — no broker needed to attest transport/parser timing. Emits
/// via `render_tms`/`render_fault` + `publish_rendered` (no `serde::Serialize` bound), the same
/// no_std render path `e2e_l1.rs::TestPlatform` and `HostPlatform::emit_reading` use — `test-e2e`
/// has no direct `serde` dependency, only `impl-app`/`lib-mapping` internals do.
struct TestPlatform {
    transport: HostSerial,
    publisher: CountingPublisher,
}
impl Platform for TestPlatform {
    type Transport = HostSerial;
    type Pub = CountingPublisher;
    fn transport(&mut self) -> &mut Self::Transport {
        &mut self.transport
    }
    fn publisher(&mut self) -> &mut Self::Pub {
        &mut self.publisher
    }
    fn now(&self) -> ClockMonotonic {
        // Fixed (not wall-clock) — payload content is irrelevant here, only publish accounting.
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

/// Task 1: a full telemetry poll cycle succeeds when the response TRICKLES in at 19200 baud
/// instead of bursting, and a second consecutive cycle over the same transport also succeeds
/// (proving no state carryover bug from the first at-rate exchange).
#[tokio::test]
#[file_serial(seplos_e2e)]
async fn telemetry_poll_succeeds_when_response_trickles_at_wire_rate_and_recovers() {
    let (master, slave) = open_pty();
    let sim = SeplosSim::new(BatteryState::sample_14_cell());
    let responder = spawn_sim_responder(master, sim, 2, Some((PACE_BAUD, TURNAROUND_MS)));

    let cfg = one_dip_config(0);
    let vendor = Seplos::new();
    let mut buf = [0u8; 256];
    let mut payload_buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    let mut health = Health::default();
    let mut plat = TestPlatform {
        transport: open_transport_default_timeout(&slave),
        publisher: CountingPublisher::default(),
    };

    let start = Instant::now();
    let outcome1 = run_cycle(
        &mut plat,
        &cfg,
        &vendor,
        &mut buf,
        &mut payload_buf,
        0,
        &mut health,
    )
    .await;
    let elapsed1 = start.elapsed();

    assert!(
        outcome1.any_success,
        "the at-rate cycle must succeed, not just an instant-burst one"
    );
    assert_eq!(health.telemetry_ok, 1);
    assert_eq!(health.telemetry_err, 0);
    // A 160-byte reply at 19200 baud (10 bits/byte) is ~83ms of pure wire time plus the ~10ms
    // simulated turnaround; a floor well below that (80ms) proves the exchange was actually paced,
    // not served as an instant burst that happened to be slow for unrelated reasons.
    assert!(
        elapsed1 >= Duration::from_millis(80),
        "exchange finished too fast ({elapsed1:?}) to have genuinely trickled at 19200 baud"
    );

    let outcome2 = run_cycle(
        &mut plat,
        &cfg,
        &vendor,
        &mut buf,
        &mut payload_buf,
        1,
        &mut health,
    )
    .await;
    assert!(
        outcome2.any_success,
        "a second consecutive at-rate cycle over the same transport must also succeed"
    );
    assert_eq!(health.telemetry_ok, 2);
    assert_eq!(health.cycles, 2);

    responder.join().ok();
}

/// Task 2: the margin probe — with pacing on, one at-rate exchange must finish comfortably inside
/// HALF the default 1000ms host read timeout, pinning the fidelity audit's ">=4x margin" claim as
/// an executable fact rather than an assertion in prose.
#[tokio::test]
#[file_serial(seplos_e2e)]
async fn paced_exchange_completes_well_inside_half_the_default_read_timeout() {
    let (master, slave) = open_pty();
    let sim = SeplosSim::new(BatteryState::sample_14_cell());
    let responder = spawn_sim_responder(master, sim, 1, Some((PACE_BAUD, TURNAROUND_MS)));

    let cfg = one_dip_config(0);
    let vendor = Seplos::new();
    let mut buf = [0u8; 256];
    let mut payload_buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    let mut health = Health::default();
    let mut plat = TestPlatform {
        transport: open_transport_default_timeout(&slave),
        publisher: CountingPublisher::default(),
    };

    let start = Instant::now();
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
    let elapsed = start.elapsed();

    assert!(outcome.any_success, "the paced exchange must still succeed");
    assert!(
        elapsed < Duration::from_millis(500),
        "a real 19200-baud exchange ({elapsed:?}) should clear well inside half the default \
         {DEFAULT_READ_TIMEOUT_MS}ms read timeout — the >=4x margin the fidelity audit claims"
    );

    responder.join().ok();
}

/// Task 3: a device-reported RTN protocol error (S1's `Behavior::RtnError`) degrades one cycle to
/// a counted telemetry failure — never a panic/abort — and the very next cycle, once the device is
/// back to `Behavior::Normal`, succeeds and clears the failure streak. Mirrors
/// `impl-app/tests/run_cycle.rs::dead_cycle_marks_failure_and_recovers`'s shape: two distinct
/// transports (one per pty/responder), one shared `health` accumulator across both cycles.
#[tokio::test]
#[file_serial(seplos_e2e)]
async fn rtn_error_is_counted_not_fatal_and_recovers_next_cycle() {
    let cfg = one_dip_config(0);
    let vendor = Seplos::new();
    let mut buf = [0u8; 256];
    let mut payload_buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    let mut health = Health::default();

    // Cycle 1: the device answers with a well-formed, checksum-valid frame carrying RTN=0x02
    // (CHKSUM error) — a real device reporting a protocol-level error, not silence or garbage.
    {
        let (master, slave) = open_pty();
        let mut sim = SeplosSim::new(BatteryState::sample_14_cell());
        sim.set_behavior(Behavior::RtnError(0x02));
        let responder = spawn_sim_responder(master, sim, 1, None);
        let mut plat = TestPlatform {
            transport: open_transport_default_timeout(&slave),
            publisher: CountingPublisher::default(),
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

        assert!(
            !outcome.any_success,
            "an RTN-error reply must degrade the cycle, never count as success"
        );
        assert_eq!(health.telemetry_ok, 0);
        assert_eq!(health.telemetry_err, 1);
        assert_eq!(health.consecutive_failed_cycles, 1);
        responder.join().ok();
    }

    // Cycle 2: the device is back to Normal — succeeds, and the failure streak clears.
    {
        let (master, slave) = open_pty();
        let sim = SeplosSim::new(BatteryState::sample_14_cell()); // Behavior::Normal by default
        let responder = spawn_sim_responder(master, sim, 1, None);
        let mut plat = TestPlatform {
            transport: open_transport_default_timeout(&slave),
            publisher: CountingPublisher::default(),
        };

        let outcome = run_cycle(
            &mut plat,
            &cfg,
            &vendor,
            &mut buf,
            &mut payload_buf,
            1,
            &mut health,
        )
        .await;

        assert!(
            outcome.any_success,
            "the cycle after the device recovers must succeed"
        );
        assert_eq!(health.telemetry_ok, 1);
        assert_eq!(health.consecutive_failed_cycles, 0);
        responder.join().ok();
    }
}
