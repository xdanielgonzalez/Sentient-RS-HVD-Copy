//! DDS/RTPS interop e2e — the executable done-bar for story #321 / WP7 (#325).
//!
//! Spawns the **actual compiled `bin-rpi` binary** (same real-binary pattern as `capture_e2e.rs` /
//! `sigterm_drain_e2e.rs` — see those files' doc comments for why this shells out to `cargo build`
//! rather than `env!("CARGO_BIN_EXE_bin-rpi")`) wired to a real pty-backed `SeplosSim` responder plus
//! a `[dds]` section, then joins the SAME MIL-STD-3071 domain-0 RTPS/UDP/IPv6 network from an
//! in-process [`lib_dds::DdsParticipant`] built the same way `test_kit::start_dds_probe`/
//! `lib-dds/tests/dds_loopback.rs` already do.
//!
//! **Single-netns design.** `bin-rpi` (a child OS process) and this test's own subscriber
//! participant are both plain processes on the SAME host, sharing ONE network namespace — there is
//! no container-per-participant split here at all (unlike `deploy/dds-conformance/`'s podman-compose
//! harness, which exists to validate against STOCK CycloneDDS tooling and needs a shared netns
//! specifically to work around per-container IPv6-multicast forwarding gaps). Running as two OS
//! processes in one netns sidesteps that class of flakiness by construction: SPDP link-local
//! multicast (`ff02::1234`) is visible to both without any compose/podman networking to get wrong.
//! The interface is `DDS_IFACE` (default `"eth0"`, matching `dds_loopback.rs`'s / the `dds-probe`
//! bin's own convention) — both participants advertise unicast DATA at loopback (`::1`, the
//! `DdsConfig::domain0` default), since same-host delivery needs no routable address.
//!
//! **What's live vs. byte-oracle vs. blocked (#332).** `lib_dds`'s writer map is PARTIAL — today only
//! `Heartbeat`, `DcMeasurementUpdate`, `DcLoadSharingState`, and the outbound `Reply` leg have a
//! dust_dds patch type (see `lib_dds::dustdds::types::patch`'s module doc). The other 7 publish
//! topics are hard-blocked on `dust_dds_derive`'s `@optional` gap (issue #332) — this file does NOT
//! assert on them; full-model XCDR2 validity for every topic (including the 7 blocked ones) is
//! already proven statically by the byte-oracle chain (`crates/lib-mapping/tests/wire_cdr_*.rs`:
//! golden `.hex` fixtures + the dust_dds cross-oracle + the CycloneDDS decode oracle from WP5/WP7a).
//! `EnergyStartStopRequest`/`PowerSwitchRequest` (the DDS-side REQUEST readers) are ALSO blocked by
//! the same #332 gap (`bin_rpi::tasks::dds::request_listener`'s subscribe calls return
//! `DdsError::UnsupportedTopic` today), so `bin-rpi`'s own DDS request-ingestion leg cannot be
//! triggered externally — the Reply/KEEP_LAST-128 scenario below therefore drives the Reply topic's
//! real writer/QoS machinery directly (the SAME `lib_dds` seam + `qos_profile(TmsTopicId::Reply)`
//! `bin-rpi` itself uses) rather than via a live DDS-encoded request, and says so.
//!
//! **Done-bar coverage in this file:**
//! - domain-0 IPv6/UDP participant join + SPDP/SEDP discovery (implicit in every live receive below
//!   — a `subscribe` cannot deliver a sample without a completed discovery handshake first).
//! - XCDR2 validity of live samples: byte-oracle-EXACT for `Heartbeat`/`DcLoadSharingState` (rendered
//!   via the SAME `lib_mapping::render_cdr::render_cdr_value` chain call production uses) plus a
//!   structural DHEADER/encapsulation self-consistency check ([`assert_valid_xcdr2_envelope`]) run on
//!   every received sample including `DcMeasurementUpdate` (whose content is real Seplos-sim-derived
//!   telemetry, not independently re-derivable here without re-implementing the whole mapping chain —
//!   see that scenario's own comment).
//! - QoS semantics: `DcLoadSharingState` is the `QosProfile::PublishLast` (TRANSIENT_LOCAL) mapped
//!   topic (`lib_tms::generated_qos::qos_profile`), so a reader created AFTER `bin-rpi` has already
//!   published proves late-joiner replay; `Reply`'s `QosProfile::Reply` (RELIABLE + VOLATILE +
//!   KEEP_LAST 128 — the exact QoS translation is unit-proven in `lib-dds/tests/seam.rs`) is exercised
//!   live for in-order, gap-free burst delivery to an already-matched reader over the real transport.
//! - `USER_DATA` = deviceId: proven by construction (every `create_writer` call in this codebase goes
//!   through `lib_dds::dustdds::qos_map::to_writer_qos`, unit-asserted in `lib-dds/tests/seam.rs`) and
//!   independently by the stock-CycloneDDS `cyclonedds ls --qos` conformance harness
//!   (`deploy/dds-conformance/`, WP7a) — `DdsSample` (this seam's subscribe delivery type) carries no
//!   participant/endpoint builtin-topic-data, so there is no additional LIVE check to add here without
//!   growing the seam itself (out of this WP's scope).
//!
//! `SENTIENT_TEST_DDS_ENDPOINT`-gated variant: when set (to an iface name reaching a real, externally
//! running DDS participant — e.g. a standalone CycloneDDS instance, not this test's own loopback
//! pairing), [`external_endpoint_smoke_test`] additionally proves `bin-rpi`'s participant is
//! discoverable and delivers at least one live sample to that external environment. Unset (the
//! default everywhere but a deliberately-configured box) => the variant logs why and returns,
//! matching `lib-dds/tests/dds_loopback.rs`'s own `SENTIENT_TEST_DDS_LOOPBACK` env-gate convention.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use nix::fcntl::OFlag;
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt, PtyMaster};
use serial_test::file_serial;

use lib_dds::dustdds::build_participant;
use lib_dds::{DdsConfig, DdsSample};
use lib_mapping::render_cdr::render_cdr_value;
use lib_mapping::schema::{ConfigId, Identity, Reply, ReplyCode, ReplyStatus, INVALID_PORT_NUMBER};
use lib_tms::generated_qos::qos_profile;
use lib_tms::{DcLoadSharingState, Heartbeat, TmsTopicId};
use test_kit::{reserve_ephemeral_port, BatteryState, SeplosSim, SimReply};

// ---- pty plumbing (mirrors `sigterm_drain_e2e.rs`'s identically-named helpers verbatim — see that
// file's doc comment for the established per-file-duplication convention this workspace follows for
// its e2e harness boilerplate) --------------------------------------------------------------------

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

const MAX_CYCLES: usize = 200;

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
    assert!(
        status.success(),
        "`cargo build -p bin-rpi` failed; cannot run dds_interop_e2e without a compiled bin-rpi"
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
        .join(format!("dds-interop-e2e-{}", std::process::id()))
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// The multicast-capable IPv6 interface both participants advertise/join on — overridable (mirrors
/// `dds_loopback.rs`/`dds-probe`'s `DDS_IFACE` convention exactly, so a dev box already configured for
/// those keeps working unchanged here).
fn dds_iface() -> String {
    std::env::var("DDS_IFACE").unwrap_or_else(|_| "eth0".to_string())
}

fn ident(s: &str) -> Identity {
    let mut i = Identity::new();
    i.push_str(s).expect("device id fits Identity (<=32 chars)");
    i
}

/// Render `value`'s complete XCDR2 SerializedPayload via the SAME production chain call
/// (`bin_rpi::tasks::dds::util::render_and_send` uses this exact function) — the byte-oracle
/// assertions below compare a live-received wire payload against THIS, not a re-derived hand rendering.
fn render<T: lib_tms::cdr::HasCdrChain>(v: &T) -> Vec<u8> {
    let mut buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    let n = render_cdr_value(v, &mut buf).expect("render fits MAX_PAYLOAD");
    buf[..n].to_vec()
}

/// Structural XCDR2 envelope self-consistency (DDS-XTypes 1.3 §7.6.3.1.2 Table 60 + §7.4.3.4.1):
/// every TMS payload is `{ D_CDR2_LE(2) + options(2) } << { DHEADER(u32) } << members << pad`, where
/// `DHEADER` is the byte length of `members` alone (excluding itself) and `pad` (0..=3, encoded in
/// `options`'s low 2 bits) brings the total to a 4-byte boundary. Returns the DHEADER `ssize` on
/// success. This is a REAL, spec-cited check computable from ANY payload without knowing its field
/// layout — used both as the encapsulation/DHEADER proof for the byte-oracle-matched topics below and
/// as the sole structural proof for `DcMeasurementUpdate` (whose content is live-telemetry-derived,
/// so an exact byte match isn't independently re-derivable here — see that scenario's own comment).
fn assert_valid_xcdr2_envelope(payload: &[u8]) -> u32 {
    assert!(
        payload.len() >= 8,
        "an XCDR2 SerializedPayload is always >= 8 bytes (4-byte encapsulation header + 4-byte \
         top-level DHEADER); got {} byte(s): {payload:02x?}",
        payload.len()
    );
    assert_eq!(
        &payload[0..2],
        &lib_tms::cdr::ENCAPSULATION_D_CDR2_LE,
        "byte 0-1 must be the D_CDR2_LE encapsulation identifier (DDS-XTypes 1.3 §7.6.3.1.2 Table 60)"
    );
    assert_eq!(
        payload[2], 0x00,
        "options byte 0 (high) is always zero for every TMS payload"
    );
    let pad = (payload[3] & 0x03) as usize;
    let ssize = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
    assert_eq!(
        8 + ssize as usize + pad,
        payload.len(),
        "DHEADER ssize ({ssize}) + the 8-byte header/DHEADER prefix + trailing pad ({pad}, from \
         options byte 3's low 2 bits) must equal the total payload length ({}); got {payload:02x?}",
        payload.len()
    );
    ssize
}

/// A fresh domain-0 `[dds]`-enabled `bin-rpi` config over `slave` (a real pty-backed Seplos sim),
/// with a fast poll cadence so `Heartbeat`/`DcMeasurementUpdate`/`DcLoadSharingState` all publish
/// within this test's bounded wait windows. Same-host loopback unicast (no `unicast_addr` override —
/// `DdsConfig::domain0`'s `::1` default is exactly right here, unlike the cross-container
/// `deploy/dds-conformance/` harness, which needs a routable link-local address).
fn dds_config_toml(slave: &str, broker_port: u16, device_id: &str) -> String {
    format!(
        r#"
[device]
id = "{device_id}"

[serial]
kind = "rs232"
port = "{slave}"
baud = 19200
read_timeout_ms = 2000

[poll]
dip_addresses = [0]
interval_ms = 150
fault_every = 0

[broker]
host = "127.0.0.1"
port = {broker_port}

[publish]
prefix = "tms"

[dds]
domain_id = 0
iface = "{iface}"
"#,
        device_id = device_id,
        slave = slave,
        broker_port = broker_port,
        iface = dds_iface(),
    )
}

/// Spawn `bin_path` against `cfg_path`, stderr drained on a background thread so a full pipe can
/// never block `bin-rpi`'s own logging (mirrors `capture_e2e.rs`/`sigterm_drain_e2e.rs`).
///
/// Each line is forwarded to THIS test process's own stderr via `eprintln!` as it is read, not just
/// accumulated into the returned `String` — a panic unwinds past the `JoinHandle::join()` that would
/// otherwise recover the accumulated string, so that string dies with the process on the panic path
/// and CI shows nothing. Forwarding live means bin-rpi's stderr always reaches the test log
/// (interleaved into CI's captured output) regardless of how the test exits; the accumulated
/// `String` is still returned for callers that want the whole blob post-hoc.
fn spawn_bin_rpi(bin_path: &Path, cfg_path: &Path) -> (BinRpiGuard, thread::JoinHandle<String>) {
    let mut child = Command::new(bin_path)
        .env("SENTIENT_CONFIG", cfg_path)
        // De-flake (CI): bin-rpi's default `info` logging emits ~200 dust_dds discovery/routing
        // span lines per second, which steals CPU on the contended 2-node ARC pool and aggravates
        // the DDS DATA-path timing race this test depends on. Quiet dust_dds to `warn` while keeping
        // bin-rpi's own `info` logs for failure debugging. (bin-rpi reads RUST_LOG via env_logger.)
        .env("RUST_LOG", "info,dust_dds=warn")
        .env_remove("CLOUD_MQTT_PASS")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bin-rpi");
    let stderr_pipe = child.stderr.take().expect("piped stderr");
    drop(child.stdout.take());
    let stderr_thread = thread::spawn(move || {
        let mut s = String::new();
        let mut reader = std::io::BufReader::new(stderr_pipe);
        let mut line = String::new();
        loop {
            line.clear();
            match std::io::BufRead::read_line(&mut reader, &mut line) {
                Ok(0) => break,
                Ok(_) => {
                    eprint!("[bin-rpi] {line}");
                    if !line.ends_with('\n') {
                        eprintln!();
                    }
                    s.push_str(&line);
                }
                Err(_) => break,
            }
        }
        s
    });
    (BinRpiGuard(child), stderr_thread)
}

/// The `Reply` (`ReplyCode::NotImplemented`) `bin_rpi::tasks::request_reply::energy_start_stop_reply`
/// would build for a well-formed `EnergyStartStopRequest` from `requester`, sequence `seq` — hand-built
/// here (not imported: `bin-rpi` is a `[[bin]]`-only crate with no library target, see
/// `request_reply_e2e.rs`'s identical note) using the real, plain `lib_mapping::schema::Reply` type.
fn sample_reply(target: &Identity, requester: &Identity, seq: u64) -> Reply {
    Reply {
        requesting_device_id: requester.clone(),
        target_device_id: target.clone(),
        config: ConfigId::Active,
        port_number: INVALID_PORT_NUMBER,
        request_sequence_id: seq,
        status: ReplyStatus {
            code: ReplyCode::NotImplemented,
            reason: {
                let mut r = lib_mapping::schema::String32::new();
                r.push_str("not implemented").expect("fits String32");
                r
            },
        },
    }
}

/// The core done-bar: real `bin-rpi` (`[dds]` on) + this test's own in-process subscriber, both on
/// domain 0 over the real dust_dds/IPv6 transport, sharing one OS network namespace.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[file_serial(seplos_e2e)]
async fn dds_interop_full_stack_over_dust_dds() {
    let bin_path = ensure_bin_rpi_built();
    let device_id = "DDS-E2E-01";

    // ---- Real pty-backed Seplos responder driving bin-rpi's poll loop (so DcMeasurementUpdate has
    // real telemetry content, and Heartbeat/DcLoadSharingState publish on their normal cadence).
    let (master, slave) = open_pty();
    let sim = SeplosSim::new(BatteryState::sample_14_cell());
    let responder = spawn_persistent_responder(master, sim);

    let broker_port = reserve_ephemeral_port();
    let cfg_dir = fresh_tmp_dir("cfg");
    let cfg_path = cfg_dir.join("sentient.toml");
    std::fs::write(&cfg_path, dds_config_toml(&slave, broker_port, device_id))
        .expect("write test config");

    let (mut guard, stderr_thread) = spawn_bin_rpi(&bin_path, &cfg_path);

    // ---- This test's OWN participant: the same one-line constructor `test_kit::start_dds_probe` /
    // `dds-probe`'s CLI / `dds_loopback.rs` use (`lib_dds::dustdds::build_participant` over
    // `DdsConfig::domain0`). Built AFTER bin-rpi is spawned but the DcLoadSharingState subscribe
    // below happens only once bin-rpi has had time to publish at least once — see the late-joiner
    // comment on that scenario.
    let config = DdsConfig::domain0(dds_iface());
    let participant = build_participant(&config).expect("build subscriber DDS participant");

    // ---- Heartbeat: byte-oracle. Content is fully deterministic from device_id + a monotonically
    // increasing sequence_number bin-rpi's own DDS heartbeat task owns; rather than guess which
    // sequence number will be first observed, render candidates for a generous bounded window and
    // assert the received payload equals ONE of them exactly (still a real chain-bytes comparison,
    // no tolerance, no re-implemented serialization).
    let (hb_tx, mut hb_rx) = tokio::sync::mpsc::channel::<DdsSample>(32);
    participant
        .subscribe(
            TmsTopicId::Heartbeat,
            "Heartbeat",
            "Heartbeat",
            qos_profile(TmsTopicId::Heartbeat),
            hb_tx,
        )
        .expect("subscribe Heartbeat");

    let hb_deadline = Instant::now() + Duration::from_secs(45);
    let got_hb = loop {
        assert!(
            Instant::now() < hb_deadline,
            "no Heartbeat sample arrived within 45s — domain-0 IPv6/UDP discovery or the real \
             dust_dds DATA path never completed; bin-rpi stderr will be printed on panic unwind"
        );
        match tokio::time::timeout(Duration::from_secs(2), hb_rx.recv()).await {
            Ok(Some(sample)) => break sample,
            _ => continue,
        }
    };
    assert_eq!(got_hb.topic, TmsTopicId::Heartbeat);
    let hb_ssize = assert_valid_xcdr2_envelope(&got_hb.payload);
    let candidates: Vec<Vec<u8>> = (0u32..60)
        .map(|seq| {
            render(&Heartbeat {
                device_id: ident(device_id),
                sequence_number: seq,
            })
        })
        .collect();
    assert!(
        candidates.iter().any(|c| c == &got_hb.payload),
        "received Heartbeat payload ({} bytes, DHEADER ssize={hb_ssize}) does not byte-match ANY \
         chain-rendered candidate sequence_number 0..60 for device {device_id:?} — got: \
         {:02x?}",
        got_hb.payload.len(),
        got_hb.payload
    );

    // ---- DcMeasurementUpdate: structural-only (real telemetry content, not independently
    // re-derivable here without reimplementing the Seplos->canonical->TMS mapping — the exact
    // byte-oracle chain proof for this topic's FULL shape already lives in
    // `lib-mapping/tests/wire_cdr_*.rs`, run unconditionally by `just test`). This is still a REAL
    // live-wire spec check: the received bytes' encapsulation header + DHEADER self-consistency,
    // over the real transport, for a topic whose content includes a rule-12 NEXTINT-delimited
    // sequence (`external_measurement`) — the exact wire shape WP7a's stock-CycloneDDS harness
    // proved CycloneDDS 11.0.1 decodes cleanly (see `lib_tms::cdr` module doc item 7).
    let (dc_tx, mut dc_rx) = tokio::sync::mpsc::channel::<DdsSample>(32);
    participant
        .subscribe(
            TmsTopicId::DcMeasurementUpdate,
            "DcMeasurementUpdate",
            "DcMeasurementUpdate",
            qos_profile(TmsTopicId::DcMeasurementUpdate),
            dc_tx,
        )
        .expect("subscribe DcMeasurementUpdate");
    let got_dc = tokio::time::timeout(Duration::from_secs(20), dc_rx.recv())
        .await
        .expect("a DcMeasurementUpdate sample arrived within 20s")
        .expect("subscriber channel open");
    assert_eq!(got_dc.topic, TmsTopicId::DcMeasurementUpdate);
    let dc_ssize = assert_valid_xcdr2_envelope(&got_dc.payload);
    assert!(
        dc_ssize > 0,
        "DcMeasurementUpdate DHEADER ssize must be > 0 (device_id + time_measured alone exceed \
         the empty case)"
    );

    // ---- DcLoadSharingState: TRANSIENT_LOCAL late-joiner replay + byte-oracle. This is the ONE
    // mapped topic on `QosProfile::PublishLast` (Durability::TransientLocal + Reliable + KeepLast(1)
    // — `lib_tms::generated_qos::qos_profile(TmsTopicId::DcLoadSharingState)`), and its content is
    // FULLY deterministic from device_id alone
    // (`bin_rpi::tasks::dds::state_publisher::dc_load_sharing_state`: `config: ConfigId::Unknown,
    // parameters: []` unconditionally), so it doubles as this file's other byte-oracle scenario. By
    // waiting here (well after bin-rpi's at-start publish) and only THEN creating the reader, the
    // eventual receipt proves REPLAY from the writer's own TRANSIENT_LOCAL history, not a lucky race
    // against the live publish.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let (ls_tx, mut ls_rx) = tokio::sync::mpsc::channel::<DdsSample>(8);
    participant
        .subscribe(
            TmsTopicId::DcLoadSharingState,
            "DcLoadSharingState",
            "DcLoadSharingState",
            qos_profile(TmsTopicId::DcLoadSharingState),
            ls_tx,
        )
        .expect("subscribe DcLoadSharingState (late join)");
    let got_ls = tokio::time::timeout(Duration::from_secs(15), ls_rx.recv())
        .await
        .expect(
            "a DcLoadSharingState sample replayed to this LATE-joining reader within 15s — \
             TRANSIENT_LOCAL history replay failed",
        )
        .expect("subscriber channel open");
    assert_eq!(got_ls.topic, TmsTopicId::DcLoadSharingState);
    assert_valid_xcdr2_envelope(&got_ls.payload);
    let expected_ls = render(&DcLoadSharingState {
        device_id: ident(device_id),
        config: ConfigId::Unknown,
        parameters: heapless::Vec::new(),
    });
    assert_eq!(
        got_ls.payload, expected_ls,
        "late-joined DcLoadSharingState replay payload must byte-match the chain rendering of the \
         deterministic {{device_id, config: Unknown, parameters: []}} value \
         (bin_rpi::tasks::dds::state_publisher::dc_load_sharing_state)"
    );

    // ---- Reply: KEEP_LAST 128 (`QosProfile::Reply` — Reliable + Volatile + KeepLast(128), unit-
    // proven in `lib-dds/tests/seam.rs::profile_policies_translate_faithfully`). bin-rpi's OWN DDS
    // request listener cannot be triggered here (EnergyStartStopRequest/PowerSwitchRequest have no
    // dust_dds patch type yet — #332, see this file's top doc comment), so this scenario drives the
    // Reply topic's real writer/QoS machinery DIRECTLY via this test's own participant (the identical
    // `lib_dds::create_writer` + `qos_profile(TmsTopicId::Reply)` call bin-rpi's
    // `tasks::dds::request_listener::handle_energy`/`handle_switch` make) — proving the wire-level
    // QoS behavior the depth enables (reliable, in-order, gap-free burst delivery to an
    // ALREADY-MATCHED reader over the real IPv6 transport), not that bin-rpi answered a live request.
    let (reply_tx, mut reply_rx) = tokio::sync::mpsc::channel::<DdsSample>(256);
    participant
        .subscribe(
            TmsTopicId::Reply,
            "Reply",
            "Reply",
            qos_profile(TmsTopicId::Reply),
            reply_tx,
        )
        .expect("subscribe Reply");
    let reply_writer = participant
        .create_writer(
            TmsTopicId::Reply,
            "Reply",
            "Reply",
            qos_profile(TmsTopicId::Reply),
            &ident(device_id),
        )
        .expect("create Reply writer");
    // Let SPDP/SEDP match the intra-participant writer<->reader pair before the burst (mirrors
    // `dds_loopback.rs`'s identical wait).
    tokio::time::sleep(Duration::from_millis(1500)).await;

    const REPLY_BURST: u64 = 20;
    let requester = ident("MC-1");
    let mut sent = Vec::with_capacity(REPLY_BURST as usize);
    for seq in 0..REPLY_BURST {
        let reply = sample_reply(&ident(device_id), &requester, seq);
        let payload = render(&reply);
        reply_writer
            .write(&ident(device_id), &payload)
            .unwrap_or_else(|e| panic!("write Reply seq={seq}: {e}"));
        sent.push(payload);
    }

    let mut received = Vec::with_capacity(REPLY_BURST as usize);
    let recv_deadline = Instant::now() + Duration::from_secs(15);
    while received.len() < REPLY_BURST as usize && Instant::now() < recv_deadline {
        if let Ok(Some(sample)) =
            tokio::time::timeout(Duration::from_secs(3), reply_rx.recv()).await
        {
            assert_eq!(sample.topic, TmsTopicId::Reply);
            assert_valid_xcdr2_envelope(&sample.payload);
            received.push(sample.payload);
        }
    }
    assert_eq!(
        received.len(),
        REPLY_BURST as usize,
        "Reply's Reliable + KeepLast(128) QoS must deliver every one of a {REPLY_BURST}-sample \
         burst gap-free to an already-matched reader; got {} of {REPLY_BURST}",
        received.len()
    );
    assert_eq!(
        received, sent,
        "received Reply payloads must byte-match (in order) the exact chain-rendered bytes written"
    );

    // ---- Teardown.
    drop(guard.0.kill());
    let _ = guard.0.wait();
    drop(guard);
    let stderr_log = stderr_thread.join().unwrap_or_default();
    responder.join().ok();
    // Only printed on an already-failed assertion path (`assert!`/`panic!` above unwinds before
    // reaching here); kept for local debugging convenience, matching `capture_e2e.rs`'s pattern of
    // surfacing bin-rpi's stderr on failure.
    let _ = stderr_log;
}

/// `SENTIENT_TEST_DDS_ENDPOINT`-gated: when set, its value is the interface reaching a real,
/// EXTERNALLY running DDS participant (not this test's own loopback pairing above) — e.g. a
/// standalone CycloneDDS instance on the same LAN segment. Proves `bin-rpi`'s participant is
/// discoverable and delivers at least one live `Heartbeat` sample into that real external
/// environment. Unset by default everywhere (including CI) — mirrors
/// `lib-dds/tests/dds_loopback.rs`'s `SENTIENT_TEST_DDS_LOOPBACK` gate convention: a loud, explained
/// skip rather than a silent no-op.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[file_serial(seplos_e2e)]
async fn external_endpoint_smoke_test() {
    let Ok(iface) = std::env::var("SENTIENT_TEST_DDS_ENDPOINT") else {
        eprintln!(
            "SKIP external_endpoint_smoke_test: set SENTIENT_TEST_DDS_ENDPOINT=<iface> (reaching a \
             real, externally running DDS participant) to run this variant — unset everywhere else, \
             including CI. The always-on `dds_interop_full_stack_over_dust_dds` above covers the \
             done-bar against this test's own loopback-paired subscriber."
        );
        return;
    };

    let bin_path = ensure_bin_rpi_built();
    let device_id = "DDS-E2E-EXT-01";

    let (master, slave) = open_pty();
    let sim = SeplosSim::new(BatteryState::sample_14_cell());
    let responder = spawn_persistent_responder(master, sim);

    let broker_port = reserve_ephemeral_port();
    let cfg_dir = fresh_tmp_dir("cfg-ext");
    let cfg_path = cfg_dir.join("sentient.toml");
    std::fs::write(
        &cfg_path,
        format!(
            r#"
[device]
id = "{device_id}"

[serial]
kind = "rs232"
port = "{slave}"
baud = 19200
read_timeout_ms = 2000

[poll]
dip_addresses = [0]
interval_ms = 150
fault_every = 0

[broker]
host = "127.0.0.1"
port = {broker_port}

[publish]
prefix = "tms"

[dds]
domain_id = 0
iface = "{iface}"
"#,
        ),
    )
    .expect("write test config");

    let (mut guard, stderr_thread) = spawn_bin_rpi(&bin_path, &cfg_path);

    let config = DdsConfig::domain0(iface);
    let participant = build_participant(&config).expect("build subscriber DDS participant");
    let (tx, mut rx) = tokio::sync::mpsc::channel::<DdsSample>(32);
    participant
        .subscribe(
            TmsTopicId::Heartbeat,
            "Heartbeat",
            "Heartbeat",
            qos_profile(TmsTopicId::Heartbeat),
            tx,
        )
        .expect("subscribe Heartbeat against the external endpoint");

    let got = tokio::time::timeout(Duration::from_secs(30), rx.recv())
        .await
        .expect(
            "a Heartbeat sample arrived from the external-endpoint-facing participant within 30s",
        )
        .expect("subscriber channel open");
    assert_eq!(got.topic, TmsTopicId::Heartbeat);
    assert_valid_xcdr2_envelope(&got.payload);

    drop(guard.0.kill());
    let _ = guard.0.wait();
    drop(guard);
    let _ = stderr_thread.join();
    responder.join().ok();
}
