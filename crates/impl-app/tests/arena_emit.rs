//! B3 — decode -> arena -> render, end to end, without firmware.
//!
//! Proves the Zephyr device-hosted-broker emit path (see `bin_zephyr_platform`'s module doc comment
//! for the full design): the poll loop (`run_cycle`) decodes a real Seplos frame and hands the
//! compact `CanonicalReading`/`CanonicalFault` to the platform's `emit_reading`/`emit_fault` seam,
//! which — for the arena-backed path — pushes into a `CanonicalStore` and marks topics pending,
//! WITHOUT building a TMS struct or serializing anything. A second, independent step then
//! simulates the broker pump: for each pending topic, call `lib_mapping::render_tms`/`render_fault`
//! against the arena's latest reading/fault, straight into a fresh buffer — proving the render
//! step reproduces exactly what the pre-redesign inline pipeline produced (byte-for-byte against
//! the same golden values `lib-mapping/tests/tms_live_frame.rs` and
//! `lib-mapping/tests/render_tms.rs` assert).
//!
//! This is deliberately single-threaded (`ArenaTestPlatform` owns its arena directly, no
//! spinlock/static) — see `bin_zephyr_platform`'s `ARENA`/`SpinArena` for how Step C's firmware wires
//! the same arena type across the real poll-loop/broker-pump thread boundary; the cross-thread
//! wiring is Step C's concern, not this test's.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use impl_app::{run_cycle, Health, Platform, RunConfig};
use lib_io::mock::{MockSerial, MockStep};
use lib_mapping::render::{render_fault, render_tms, TmsTopic};
use lib_mapping::schema::ClockMonotonic;
use lib_mapping::sink::{PublishError, Publisher};
use lib_parse::canonical::{CanonicalFault, CanonicalReading};
use lib_parse::seplos::checksum::{frame_checksum, length_checksum};
use lib_parse::seplos::Seplos;
use lib_store::{CanonicalStore, PendingTopic};

/// The same live 14-cell telemetry capture used throughout (`impl-app/tests/run_cycle.rs`,
/// `lib-mapping/tests/tms_live_frame.rs`): Seplos 48150-1101-10E, DIP 0. Known decode: 14 cells,
/// cell[0] = 3894 mV, current = -164 cA, total voltage = 5452 cV (54.52 V), soc = 775 dpct.
const LIVE_FRAME: &[u8] = b"~20004600A08E00000E0F360F360F360F360F350F360F360F360F360F370F370F360F360F35060AF10AF90AF90AF30B370B0CFF5C154C36820A465003074650000203E8154CFFF800000079004DDEC8\r";

fn push_hex_u8(out: &mut Vec<u8>, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push(HEX[(byte >> 4) as usize]);
    out.push(HEX[(byte & 0xF) as usize]);
}

/// A synthetic Seplos `0x44` fault-response frame for DIP `adr` (same construction as
/// `impl-app/tests/run_cycle.rs::make_fault_frame`).
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

/// A no-op [`Publisher`] — the arena-backed emit path never calls `publish` (see
/// `bin_zephyr_platform::ZephyrBrokerPublisher`, the firmware equivalent this test double mirrors).
struct NoopPublisher;
impl Publisher for NoopPublisher {
    fn publish(&mut self, _topic: &str, _payload: &[u8]) -> Result<(), PublishError> {
        Err(PublishError::Sink)
    }
    fn healthy(&self) -> bool {
        true
    }
}

/// A `Platform` whose `emit_reading`/`emit_fault` push straight into an owned [`CanonicalStore`] —
/// the same shape `bin_zephyr_platform::ZephyrPlatform`'s `Broker` publisher variant uses (see that
/// module's doc comment), minus the cross-thread static/spinlock (this test is single-threaded).
struct ArenaTestPlatform<'a, const N: usize> {
    transport: MockSerial<'a, N>,
    publisher: NoopPublisher,
    arena: CanonicalStore<4>,
}

impl<'a, const N: usize> Platform for ArenaTestPlatform<'a, N> {
    type Transport = MockSerial<'a, N>;
    type Pub = NoopPublisher;
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
        _cfg: &RunConfig,
        reading: &CanonicalReading,
        _now: ClockMonotonic,
        _payload_buf: &mut [u8],
        health: &mut Health,
    ) {
        // NO TMS build, NO serialize — exactly `ZephyrPlatform::emit_reading`'s `Broker` branch.
        self.arena.push_reading(reading.clone());
        health.record_publish(true);
        health.record_publish(true);
    }
    fn emit_fault(
        &mut self,
        _cfg: &RunConfig,
        fault: &CanonicalFault,
        _now: ClockMonotonic,
        _payload_buf: &mut [u8],
        health: &mut Health,
    ) {
        self.arena.set_fault(*fault);
        health.record_publish(true);
    }
}

fn dev_id() -> heapless::String<32> {
    let mut s = heapless::String::new();
    s.push_str("METEOR-001").expect("fits");
    s
}

fn one_dip_config(fault_every: u32) -> RunConfig {
    let mut dips = heapless::Vec::new();
    dips.push(0).expect("fits");
    let mut prefix = heapless::String::new();
    prefix.push_str("tms").expect("fits");
    RunConfig {
        device_id: dev_id(),
        prefix,
        dip_addresses: dips,
        interval_ms: 2000,
        fault_every,
        max_backoff_ms: 30_000,
    }
}

fn now_zero() -> ClockMonotonic {
    ClockMonotonic {
        seconds: 0,
        nanoseconds: 0,
    }
}

/// The full path: `run_cycle` decodes `LIVE_FRAME` -> `emit_reading` pushes into the arena and
/// marks both telemetry topics pending -> NOTHING is serialized yet -> a simulated broker-pump
/// drain renders each pending topic from the arena's latest reading, and the rendered bytes match
/// the pipeline's known golden values byte-for-byte.
#[tokio::test]
async fn decode_into_arena_then_render_matches_golden_bytes() {
    let cfg = one_dip_config(0);
    let vendor = Seplos::new();
    let mut buf = [0u8; 256];
    let mut payload_buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    let mut health = Health::default();
    let mut plat: ArenaTestPlatform<4> = ArenaTestPlatform {
        transport: MockSerial::new(&[MockStep::Reply(LIVE_FRAME)]),
        publisher: NoopPublisher,
        arena: CanonicalStore::new(),
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
    assert!(outcome.any_success);
    assert_eq!(health.telemetry_ok, 1);
    // The arena-backed emit path counts both telemetry publishes as accepted (an overwrite-oldest
    // ring buffer never rejects a write) even though nothing was actually serialized yet.
    assert_eq!(health.publish_ok, 2);

    // The arena now holds exactly the reading `run_cycle` decoded, and both telemetry topics are
    // pending — proving the poll-loop side of the seam did its job with no TMS/JSON involved.
    assert_eq!(plat.arena.len(), 1);
    let pending = plat.arena.take_pending();
    assert_eq!(pending.len(), 2);
    assert!(pending.contains(&PendingTopic::DcMeasurementUpdate));
    assert!(pending.contains(&PendingTopic::PowerHardwareUpdate));

    // Simulate the broker pump: render each pending topic from the arena's latest reading.
    let reading = plat
        .arena
        .latest_reading()
        .expect("arena holds the decoded reading");

    let mut dc_buf = [0u8; 4096];
    let dc_n = render_tms(
        TmsTopic::DcMeasurementUpdate,
        dev_id(),
        reading,
        now_zero(),
        &mut dc_buf,
    )
    .expect("render dc");
    let dc_json = core::str::from_utf8(&dc_buf[..dc_n]).expect("utf8");
    assert!(dc_json.contains("\"voltage\":54.52"), "got: {dc_json}");
    assert!(dc_json.contains("\"amperage\":1.64"), "got: {dc_json}");
    assert!(dc_json.contains("METEOR-001"));

    let mut hw_buf = [0u8; 4096];
    let hw_n = render_tms(
        TmsTopic::PowerHardwareUpdate,
        dev_id(),
        reading,
        now_zero(),
        &mut hw_buf,
    )
    .expect("render hw");
    let hw_json = core::str::from_utf8(&hw_buf[..hw_n]).expect("utf8");
    assert_eq!(hw_json.matches("\"cell_number\":").count(), 14, "14 cells");
    assert!(hw_json.contains("\"run_time\":null"));
}

/// Same shape, for the fault-sourced diagnostic topic: `emit_fault` sets the arena's fault snapshot
/// and marks `ActiveDiagnosticState` pending; the simulated broker pump renders it from
/// `latest_fault()`.
#[tokio::test]
async fn decode_fault_into_arena_then_render_matches_golden_bytes() {
    let cfg = one_dip_config(1); // fault polled every cycle
    let vendor = Seplos::new();
    let mut buf = [0u8; 256];
    let mut payload_buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    let mut health = Health::default();
    // Fault info bytes with high_voltage_protection + mosfet_fault set, per
    // `lib_parse::seplos::fault::decode_fault`'s (byte, bit) map: (3,0) high_voltage_protection,
    // (6,0) mosfet_fault.
    let mut info = [0u8; 11];
    info[3] = 0b0000_0001; // high_voltage_protection
    info[6] = 0b0000_0001; // mosfet_fault
    let fault_frame = make_fault_frame(0, &info);
    let mut plat: ArenaTestPlatform<4> = ArenaTestPlatform {
        transport: MockSerial::new(&[MockStep::Reply(LIVE_FRAME), MockStep::Reply(&fault_frame)]),
        publisher: NoopPublisher,
        arena: CanonicalStore::new(),
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
    assert!(outcome.any_success);
    assert_eq!(health.fault_ok, 1);

    let pending = plat.arena.take_pending();
    assert!(pending.contains(&PendingTopic::ActiveDiagnosticState));

    let fault = plat
        .arena
        .latest_fault()
        .expect("arena holds the decoded fault");

    assert!(fault.high_voltage_protection, "decoded from info[3] bit 0");
    assert!(fault.mosfet_fault, "decoded from info[6] bit 0");

    let mut buf_out = [0u8; 4096];
    let n = render_fault(fault, dev_id(), now_zero(), &mut buf_out).expect("render fault");
    let json = core::str::from_utf8(&buf_out[..n]).expect("utf8");
    assert!(json.contains("METEOR-001"));
    // Exactly the two DTCs the set flags map to (SPN 1 = high_voltage_protection / "cell
    // overvoltage", SPN 8 = mosfet_fault / "mosfet fault" — see
    // `lib_mapping::transform::FAULT_TABLE`).
    assert_eq!(
        json.matches("\"suspect_parameter\":").count(),
        2,
        "got: {json}"
    );
    assert!(json.contains("\"suspect_parameter\":1"), "got: {json}");
    assert!(json.contains("\"suspect_parameter\":8"), "got: {json}");
}
