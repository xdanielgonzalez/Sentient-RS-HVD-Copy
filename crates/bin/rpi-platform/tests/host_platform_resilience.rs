//! Bug #172 — serial startup resilience (host platform), updated for the #238/#242 multi-port
//! reshape: `HostPlatform` no longer owns the embedded broker (hoisted out to the caller — see
//! `HostPlatform`'s doc comment) or feed the systemd watchdog directly (moved to
//! `host::WatchdogAgg::run`, aggregating one `CyclePulse` per bus/background task).
//!
//! `HostPlatform::build_port` must NOT hard-fail when the configured serial port cannot be
//! opened: a device with an unplugged/faulty/busy RS-485 dongle (or one wired to a console UART
//! -> EPERM) must start DEGRADED — transport `Disconnected` (see
//! `lib_io::host::HostSerial::disconnected`) — instead of exiting. Recovery is
//! `Platform::reconnect`'s existing backoff loop; that path is exercised by
//! `impl-app/tests/run_cycle.rs`, not here.
//!
//! The companion regression is the watchdog feed: `on_cycle` must pet its `CyclePulse`
//! UNCONDITIONALLY — regardless of `Health::healthy()` — or a persistently-disconnected serial
//! would starve the aggregated watchdog feed and systemd would restart a perfectly live,
//! self-healing process (swapping the old exit-1 crash-loop for a watchdog crash-loop). A second
//! test drives `WatchdogAgg::run` itself end to end (real `NOTIFY_SOCKET` datagram) to prove the
//! aggregated feed still reaches systemd once `on_cycle`'s pulse is flowing.
//!
//! Both integration-style: `HostPlatform::build_port` against a real embedded broker (on a
//! scavenged ephemeral port via `test_kit::mqtt::start_broker`'s sibling helper) and a real
//! (deliberately unopenable) `HostSerial::open`. Unix only — same gating as
//! `lib-io/tests/host_timeout.rs`. This crate (`bin-rpi-platform`) is host-only, unconditionally
//! (no features), so no `feature = "host"` gate is needed here anymore (ADR 0009).
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::env;
use std::os::unix::net::UnixDatagram;
use std::time::Duration;

use bin_rpi_platform::{
    CyclePulse, DirectionMode, HostPlatform, PreflightInputs, SinkSet, WatchdogAgg,
};
use lib_io::host::SerialKind;
use lib_io::transport::SerialParams;
use lib_mqtt::broker::{BrokerConfig, LocalBroker};
use spec_app::{Health, Platform, UNHEALTHY_AFTER_CYCLES};

/// Reserve a free ephemeral TCP port the same way `test_kit::mqtt::start_broker` does, so the
/// embedded broker in these tests never fights another test (or CI run) for a fixed port.
fn reserve_ephemeral_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind an ephemeral port")
        .local_addr()
        .expect("read back the OS-assigned local address")
        .port()
}

fn test_preflight_inputs() -> PreflightInputs {
    PreflightInputs {
        device_id_present: true,
        dip_addresses_present: true,
        cloud: None,
    }
}

/// Start a real embedded broker on an ephemeral port and mint one publisher off it — the shared
/// setup every test here needs now that `HostPlatform` no longer owns the broker itself.
fn start_test_broker() -> LocalBroker {
    let port = reserve_ephemeral_port();
    let broker_cfg = BrokerConfig {
        id: "sentient-test-resilience".into(),
        host: "127.0.0.1".into(),
        port,
        auth: None,
    };
    LocalBroker::start(broker_cfg).expect("start test broker")
}

/// `HostPlatform::build_port` against a serial path that can never be opened (nonexistent device
/// node) must return `Ok`, not propagate the open failure — the platform must still come up
/// DEGRADED.
#[tokio::test]
async fn build_port_with_unopenable_port_starts_degraded_not_err() {
    let mut broker = start_test_broker();

    let result = HostPlatform::build_port(
        broker.publisher(),
        SinkSet::default(),
        "test-bus".to_string(),
        "/dev/tty-sentient-does-not-exist-anywhere",
        SerialKind::Rs232,
        SerialParams::default(),
        DirectionMode::default(),
        100,
        CyclePulse::default(),
        test_preflight_inputs(),
        None,
        Duration::from_secs(10),
    );

    let mut platform = match result {
        Ok(p) => p,
        Err(e) => panic!("build_port() must be non-fatal on a serial-open failure, got Err: {e}"),
    };

    // The transport is the `Disconnected` placeholder: every read fails fast with `Timeout` rather
    // than hanging or panicking — proof the DEGRADED transport, not a live one, is in place.
    use lib_io::transport::{IoError, SerialTransport};
    let mut buf = [0u8; 16];
    assert_eq!(
        platform.transport().read(&mut buf).await,
        Err(IoError::Timeout),
        "a degraded-start transport must be the Disconnected placeholder"
    );
}

/// `HostPlatform::on_cycle` pets this bus's `CyclePulse` on every call — UNCONDITIONALLY,
/// independent of `Health::healthy()`. This is the regression bug #172 calls out explicitly:
/// gating the liveness feed on `healthy()` meant a persistently-disconnected serial (3+ failed
/// cycles) would stop feeding the watchdog and systemd would restart (crash-loop) an otherwise
/// live, self-healing process. Since the aggregated watchdog feed (`WatchdogAgg::run`, exercised
/// separately below) only ever reads the pulse's counter, proving the counter advances regardless
/// of `health` proves the same liveness guarantee bug #172 fixed, without needing a live
/// `NOTIFY_SOCKET` here.
#[tokio::test]
async fn on_cycle_pets_cycle_pulse_even_when_unhealthy() {
    let mut broker = start_test_broker();
    let pulse = CyclePulse::default();

    let mut platform = HostPlatform::build_port(
        broker.publisher(),
        SinkSet::default(),
        "test-bus".to_string(),
        "/dev/tty-sentient-does-not-exist-anywhere",
        SerialKind::Rs232,
        SerialParams::default(),
        DirectionMode::default(),
        100,
        pulse.clone(),
        test_preflight_inputs(),
        None,
        Duration::from_secs(10),
    )
    .expect("build_port() must be non-fatal on a serial-open failure");

    // Drive health past UNHEALTHY_AFTER_CYCLES failed cycles — healthy() is now false.
    let mut health = Health::default();
    for _ in 0..UNHEALTHY_AFTER_CYCLES {
        health.end_cycle(false);
    }
    assert!(
        !health.healthy(),
        "test setup: health must be unhealthy before asserting the pulse still advances"
    );

    let before = pulse.get();
    platform.on_cycle(&health);
    assert_eq!(
        pulse.get(),
        before + 1,
        "on_cycle must pet the CyclePulse even though health.healthy() == false"
    );
}

/// `WatchdogAgg::run` feeds the real systemd watchdog (`NOTIFY_SOCKET`) once its registered
/// `CyclePulse` has pulsed at least once inside `window_ms` — the aggregated feed
/// `on_cycle_pets_cycle_pulse_even_when_unhealthy` proves the pulse side of, exercised end to end.
///
/// `NOTIFY_SOCKET` is process-global env, so this test owns a dedicated socket file name and
/// cleans it up on every exit path; the real 1 s ticker inside `WatchdogAgg::run` makes this a
/// genuinely time-based (not mocked-clock) integration test.
#[tokio::test]
async fn watchdog_agg_run_feeds_watchdog_once_pulse_is_registered() {
    let socket_path = env::temp_dir().join(format!(
        "sentient-test-watchdog-agg-{}.sock",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&socket_path);
    let sock = UnixDatagram::bind(&socket_path).expect("bind a NOTIFY_SOCKET test socket");
    sock.set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set a read timeout so a missing datagram fails the test, not hangs it");

    let prev = env::var_os("NOTIFY_SOCKET");
    env::set_var("NOTIFY_SOCKET", &socket_path);

    let mut watchdog = WatchdogAgg::default();
    let pulse = watchdog.register("test-task");
    pulse.pet();
    tokio::spawn(watchdog.run(60_000));

    let result = tokio::task::spawn_blocking({
        let sock = sock.try_clone().expect("clone socket for blocking recv");
        move || {
            let mut buf = [0u8; 1024];
            sock.recv(&mut buf).map(|len| (buf, len))
        }
    })
    .await
    .expect("blocking recv task");

    match prev {
        Some(v) => env::set_var("NOTIFY_SOCKET", v),
        None => env::remove_var("NOTIFY_SOCKET"),
    }
    let _ = std::fs::remove_file(&socket_path);

    let (buf, len) = result.expect("expected a WATCHDOG=1 datagram from the aggregated feed");
    let msg = String::from_utf8_lossy(&buf[..len]);
    assert!(
        msg.contains("WATCHDOG=1"),
        "aggregated watchdog feed must send WATCHDOG=1, got: {msg:?}"
    );
}
