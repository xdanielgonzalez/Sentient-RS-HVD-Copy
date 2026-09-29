//! T4a — the poll cycle degrades gracefully and accounts for every outcome.
//!
//! Drives [`impl_app::run_cycle`] over a scripted [`MockSerial`] transport and a counting
//! publisher (no broker, no wire), asserting: a good telemetry frame counts two publishes and
//! resets the failure streak; a publish-sink failure is counted and does not abort the cycle; an
//! all-DIP failure yields `any_success == false` and increments the consecutive-failure streak;
//! recovery clears it. Also covers the pure [`Backoff`] and [`Health`] logic, the `fault_every`
//! cadence (both a decoded and a failed fault exchange), and the outer [`impl_app::run`] loop
//! (success/dead-cycle/backoff/reconnect via the `Platform` default methods).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use impl_app::{run_cycle, Backoff, Health, Platform, RunConfig, UNHEALTHY_AFTER_CYCLES};
use lib_io::mock::{MockSerial, MockStep};
use lib_mapping::schema::{topics, ClockMonotonic};
use lib_mapping::sink::{publish_tms_into, PublishError, Publisher};
use lib_mapping::transform::{ToDiagnostics, ToTms};
use lib_parse::canonical::{CanonicalFault, CanonicalReading};
use lib_parse::seplos::checksum::{frame_checksum, length_checksum};
use lib_parse::seplos::Seplos;

/// Build+publish both telemetry TMS payloads and record the outcome — the same shape
/// `HostPlatform::emit_reading` uses, reproduced here so these tests keep exercising `run_cycle`'s
/// publish-accounting behavior through the new `Platform::emit_reading` seam (a test double is not
/// `HostPlatform`, so it can't reuse that impl directly).
fn emit_reading_via_publish<P: Platform>(
    platform: &mut P,
    cfg: &RunConfig,
    reading: &CanonicalReading,
    now: ClockMonotonic,
    payload_buf: &mut [u8],
    health: &mut Health,
) {
    let dc = reading.to_dc_measurement(cfg.device_id.clone(), now);
    publish_counted(
        platform,
        health,
        &cfg.prefix,
        &cfg.device_id,
        topics::DC_MEASUREMENT_UPDATE,
        &dc,
        payload_buf,
    );
    let hw = reading.to_power_hardware(cfg.device_id.clone(), now);
    publish_counted(
        platform,
        health,
        &cfg.prefix,
        &cfg.device_id,
        topics::POWER_HARDWARE_UPDATE,
        &hw,
        payload_buf,
    );
}

/// Build+publish `ActiveDiagnosticState` and record the outcome — see
/// `emit_reading_via_publish`'s doc comment.
fn emit_fault_via_publish<P: Platform>(
    platform: &mut P,
    cfg: &RunConfig,
    fault: &CanonicalFault,
    now: ClockMonotonic,
    payload_buf: &mut [u8],
    health: &mut Health,
) {
    let ds = fault.to_active_diagnostics(cfg.device_id.clone(), now);
    publish_counted(
        platform,
        health,
        &cfg.prefix,
        &cfg.device_id,
        topics::ACTIVE_DIAGNOSTIC_STATE,
        &ds,
        payload_buf,
    );
}

fn publish_counted<P: Platform, T: serde::Serialize>(
    platform: &mut P,
    health: &mut Health,
    prefix: &str,
    device_id: &heapless::String<32>,
    topic: &str,
    payload: &T,
    payload_buf: &mut [u8],
) {
    match publish_tms_into(
        platform.publisher(),
        prefix,
        device_id,
        topic,
        payload,
        payload_buf,
    ) {
        Ok(()) => health.record_publish(true),
        Err(_e) => health.record_publish(false),
    }
}

/// The live 14-cell telemetry capture (Seplos 48150-1101-10E, DIP 0) — decodes to a reading.
const LIVE_FRAME: &[u8] = b"~20004600A08E00000E0F360F360F360F360F350F360F360F360F360F370F370F360F360F35060AF10AF90AF90AF30B370B0CFF5C154C36820A465003074650000203E8154CFFF800000079004DDEC8\r";

fn push_hex_u8(out: &mut Vec<u8>, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push(HEX[(byte >> 4) as usize]);
    out.push(HEX[(byte & 0xF) as usize]);
}

/// Assemble a byte-exact synthetic Seplos `0x44` fault-response frame for DIP `adr` (same
/// construction as `lib-parse`'s T7 fault vector — duplicated here rather than shared across
/// the crate boundary, since integration test binaries can't depend on another crate's test-only
/// helpers).
fn make_fault_frame(adr: u8, info: &[u8]) -> Vec<u8> {
    let lenid = (info.len() * 2) as u16;
    let length: u16 = ((length_checksum(lenid) as u16) << 12) | (lenid & 0x0FFF);

    let mut body = Vec::new();
    body.extend_from_slice(b"20");
    push_hex_u8(&mut body, adr);
    body.extend_from_slice(b"46");
    push_hex_u8(&mut body, 0x00);
    push_hex_u8(&mut body, (length >> 8) as u8);
    push_hex_u8(&mut body, (length & 0xFF) as u8);
    for &b in info {
        push_hex_u8(&mut body, b);
    }

    let cs = frame_checksum(&body);
    let mut frame = Vec::new();
    frame.push(b'~');
    frame.extend_from_slice(&body);
    push_hex_u8(&mut frame, (cs >> 8) as u8);
    push_hex_u8(&mut frame, (cs & 0xFF) as u8);
    frame.push(b'\r');
    frame
}

/// A publisher that counts calls and can be forced to fail, to exercise publish accounting.
struct CountingPublisher {
    calls: u32,
    fail: bool,
}
impl Publisher for CountingPublisher {
    fn publish(&mut self, _topic: &str, _payload: &[u8]) -> Result<(), PublishError> {
        self.calls += 1;
        if self.fail {
            Err(PublishError::Sink)
        } else {
            Ok(())
        }
    }
    fn healthy(&self) -> bool {
        !self.fail
    }
}

/// A test platform: scripted transport + counting publisher + a fixed clock.
struct TestPlatform<'a, const N: usize> {
    transport: MockSerial<'a, N>,
    publisher: CountingPublisher,
}
impl<'a, const N: usize> Platform for TestPlatform<'a, N> {
    type Transport = MockSerial<'a, N>;
    type Pub = CountingPublisher;
    fn transport(&mut self) -> &mut Self::Transport {
        &mut self.transport
    }
    fn publisher(&mut self) -> &mut Self::Pub {
        &mut self.publisher
    }
    fn now(&self) -> ClockMonotonic {
        ClockMonotonic {
            seconds: 0,
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
        emit_reading_via_publish(self, cfg, reading, now, payload_buf, health);
    }
    fn emit_fault(
        &mut self,
        cfg: &RunConfig,
        fault: &CanonicalFault,
        now: ClockMonotonic,
        payload_buf: &mut [u8],
        health: &mut Health,
    ) {
        emit_fault_via_publish(self, cfg, fault, now, payload_buf, health);
    }
}

fn one_dip_config() -> RunConfig {
    one_dip_config_with_fault_every(0)
}

/// Same single-DIP config as [`one_dip_config`], but polling faults every `fault_every` cycles
/// (0 disables, matching the default).
fn one_dip_config_with_fault_every(fault_every: u32) -> RunConfig {
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

fn platform_with<'a, const N: usize>(
    steps: &[MockStep<'a>],
    fail_publish: bool,
) -> TestPlatform<'a, N> {
    TestPlatform {
        transport: MockSerial::new(steps),
        publisher: CountingPublisher {
            calls: 0,
            fail: fail_publish,
        },
    }
}

#[tokio::test]
async fn good_telemetry_counts_two_publishes_and_succeeds() {
    let cfg = one_dip_config();
    let vendor = Seplos::new();
    let mut buf = [0u8; 256];
    let mut payload_buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    let mut health = Health::default();
    let mut plat: TestPlatform<4> = platform_with(&[MockStep::Reply(LIVE_FRAME)], false);

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

    assert!(outcome.any_success);
    assert_eq!(health.telemetry_ok, 1);
    assert_eq!(health.telemetry_err, 0);
    // DcMeasurementUpdate + PowerHardwareUpdate = two publishes.
    assert_eq!(health.publish_ok, 2);
    assert_eq!(plat.publisher.calls, 2);
    assert_eq!(health.consecutive_failed_cycles, 0);
    assert_eq!(health.cycles, 1);
}

#[tokio::test]
async fn publish_sink_failure_is_counted_not_fatal() {
    let cfg = one_dip_config();
    let vendor = Seplos::new();
    let mut buf = [0u8; 256];
    let mut payload_buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    let mut health = Health::default();
    let mut plat: TestPlatform<4> = platform_with(&[MockStep::Reply(LIVE_FRAME)], true);

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

    // The exchange still succeeded (telemetry decoded); only the sink failed.
    assert!(outcome.any_success);
    assert_eq!(health.telemetry_ok, 1);
    assert_eq!(health.publish_ok, 0);
    assert_eq!(health.publish_err, 2);
    assert_eq!(
        plat.publisher.calls, 2,
        "both publishes attempted despite the first failing"
    );
}

#[tokio::test]
async fn dead_cycle_marks_failure_and_recovers() {
    let cfg = one_dip_config();
    let vendor = Seplos::new();
    let mut buf = [0u8; 256];
    let mut payload_buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    let mut health = Health::default();

    // Cycle 1: silent bus → telemetry error, no success.
    let mut dead: TestPlatform<4> = platform_with(&[MockStep::Timeout], false);
    let o1 = run_cycle(
        &mut dead,
        &cfg,
        &vendor,
        &mut buf,
        &mut payload_buf,
        0,
        &mut health,
    )
    .await;
    assert!(!o1.any_success);
    assert_eq!(health.telemetry_err, 1);
    assert_eq!(health.consecutive_failed_cycles, 1);

    // Cycle 2: good frame → success resets the streak.
    let mut good: TestPlatform<4> = platform_with(&[MockStep::Reply(LIVE_FRAME)], false);
    let o2 = run_cycle(
        &mut good,
        &cfg,
        &vendor,
        &mut buf,
        &mut payload_buf,
        1,
        &mut health,
    )
    .await;
    assert!(o2.any_success);
    assert_eq!(health.consecutive_failed_cycles, 0);
    assert_eq!(health.cycles, 2);
}

#[test]
fn backoff_doubles_to_cap_then_resets() {
    let mut b = Backoff::new();
    assert_eq!(b.next_delay(), 1_000);
    assert_eq!(b.next_delay(), 2_000);
    assert_eq!(b.next_delay(), 4_000);
    assert_eq!(b.next_delay(), 8_000);
    assert_eq!(b.next_delay(), 16_000);
    assert_eq!(b.next_delay(), 30_000); // 32_000 capped
    assert_eq!(b.next_delay(), 30_000); // stays at cap
    b.reset();
    assert_eq!(b.next_delay(), 1_000);
}

#[test]
fn health_unhealthy_after_streak() {
    let mut h = Health::default();
    for _ in 0..UNHEALTHY_AFTER_CYCLES {
        assert!(h.healthy());
        h.end_cycle(false);
    }
    assert!(
        !h.healthy(),
        "unhealthy once the streak reaches the threshold"
    );
    h.end_cycle(true);
    assert!(h.healthy(), "one success restores health");
}

#[test]
fn backoff_default_matches_new() {
    assert_eq!(Backoff::default(), Backoff::new());
}

#[test]
fn health_record_fault_counts_ok_and_err() {
    let mut h = Health::default();
    h.record_fault(true);
    h.record_fault(true);
    h.record_fault(false);
    assert_eq!(h.fault_ok, 2);
    assert_eq!(h.fault_err, 1);
}

#[tokio::test]
async fn fault_due_decoded_fault_publishes_diagnostic() {
    let cfg = one_dip_config_with_fault_every(1); // fault polled every cycle
    let vendor = Seplos::new();
    let mut buf = [0u8; 256];
    let mut payload_buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    let mut health = Health::default();
    // All-clear fault info still decodes to a CanonicalFault (with every flag false).
    let fault_frame = make_fault_frame(0, &[0u8; 11]);
    let mut plat: TestPlatform<4> = platform_with(
        &[MockStep::Reply(LIVE_FRAME), MockStep::Reply(&fault_frame)],
        false,
    );

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

    assert!(outcome.any_success);
    assert_eq!(health.fault_ok, 1);
    assert_eq!(health.fault_err, 0);
    // DcMeasurementUpdate + PowerHardwareUpdate (telemetry) + ActiveDiagnosticState (fault) = 3.
    assert_eq!(health.publish_ok, 3);
    assert_eq!(plat.publisher.calls, 3);
}

#[tokio::test]
async fn fault_due_failed_fault_is_counted_not_fatal() {
    let cfg = one_dip_config_with_fault_every(1); // fault polled every cycle
    let vendor = Seplos::new();
    let mut buf = [0u8; 256];
    let mut payload_buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    let mut health = Health::default();
    let mut plat: TestPlatform<4> =
        platform_with(&[MockStep::Reply(LIVE_FRAME), MockStep::Timeout], false);

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

    // Telemetry still succeeded; only the fault exchange (second scripted step) timed out.
    assert!(outcome.any_success);
    assert_eq!(health.telemetry_ok, 1);
    assert_eq!(health.fault_ok, 0);
    assert_eq!(health.fault_err, 1);
    // Only the two telemetry publishes went out; the fault decode never reached publish_counted.
    assert_eq!(health.publish_ok, 2);
    assert_eq!(plat.publisher.calls, 2);
}

/// A `Platform` whose `sleep` actually yields to the executor (`tokio::task::yield_now`), unlike
/// [`TestPlatform`]'s no-op — needed so the outer [`impl_app::run`] loop below can be aborted
/// from outside instead of spinning through an unbounded number of immediately-ready futures
/// inside one poll. `on_cycle`/`reconnect` are deliberately left at the `Platform` trait defaults:
/// that's exactly the code path this test exercises.
struct YieldingPlatform<'a, const N: usize> {
    transport: MockSerial<'a, N>,
    publisher: CountingPublisher,
}
impl<'a, const N: usize> Platform for YieldingPlatform<'a, N> {
    type Transport = MockSerial<'a, N>;
    type Pub = CountingPublisher;
    fn transport(&mut self) -> &mut Self::Transport {
        &mut self.transport
    }
    fn publisher(&mut self) -> &mut Self::Pub {
        &mut self.publisher
    }
    fn now(&self) -> ClockMonotonic {
        ClockMonotonic {
            seconds: 0,
            nanoseconds: 0,
        }
    }
    async fn sleep(&mut self, _ms: u32) {
        tokio::task::yield_now().await;
    }
    fn emit_reading(
        &mut self,
        cfg: &RunConfig,
        reading: &CanonicalReading,
        now: ClockMonotonic,
        payload_buf: &mut [u8],
        health: &mut Health,
    ) {
        emit_reading_via_publish(self, cfg, reading, now, payload_buf, health);
    }
    fn emit_fault(
        &mut self,
        cfg: &RunConfig,
        fault: &CanonicalFault,
        now: ClockMonotonic,
        payload_buf: &mut [u8],
        health: &mut Health,
    ) {
        emit_fault_via_publish(self, cfg, fault, now, payload_buf, health);
    }
}

#[tokio::test]
async fn run_loop_drives_success_dead_backoff_and_reconnect() {
    let cfg = one_dip_config();
    let plat: YieldingPlatform<8> = YieldingPlatform {
        // One good cycle (the `any_success` sleep branch), then enough dead cycles to cross
        // `RECONNECT_AFTER_CYCLES` (backing off + attempting `reconnect`, which defaults to
        // `false`); the script then exhausts and every further cycle stays silently dead, which
        // (since `sleep` here only yields rather than actually waiting) runs enough additional
        // cycles in our short real-time budget to also cross a `SUMMARY_EVERY_CYCLES` boundary.
        transport: MockSerial::new(&[
            MockStep::Reply(LIVE_FRAME),
            MockStep::Timeout,
            MockStep::Timeout,
            MockStep::Timeout,
            MockStep::Timeout,
        ]),
        publisher: CountingPublisher {
            calls: 0,
            fail: false,
        },
    };

    let handle = tokio::spawn(impl_app::run(plat, cfg));
    tokio::time::sleep(Duration::from_millis(50)).await;
    handle.abort();

    match handle.await {
        Ok(never) => match never {},
        Err(e) => assert!(
            e.is_cancelled(),
            "the loop must only ever stop via our abort, never panic: {e:?}"
        ),
    }
}
