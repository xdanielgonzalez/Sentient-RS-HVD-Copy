//! Cucumber (Gherkin) spec-alignment suite runner — Seplos -> Canonical -> TMS (MIL-STD-3071)
//! field correctness, in-process, reusing the real production/testkit code (zero
//! re-implementation, zero sim/decoder drift). See `tests/steps/mod.rs` for the step definitions
//! and `tests/features/*.feature` for the human-readable spec-alignment corpus itself.
//!
//! `harness = false` (see `Cargo.toml`'s `[[test]]`): this binary IS the test harness (cucumber
//! prints its own progress/summary instead of libtest's), so it is intentionally NOT
//! nextest-runnable — run it via `just test-acceptance` / `cargo test -p test-acceptance
//! --locked --test cucumber`, never wired into the `test-e2e` nextest lane.
//!
//! Test code only; freely uses unwrap/expect/panic (the workspace lint gate denies these in
//! production logic, not tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod steps;

use cucumber::World as _;
use heapless::String as HString;
use lib_mapping::schema::{
    ActiveDiagnosticState, ClockMonotonic, DcMeasurementUpdate, PowerHardwareUpdate,
};
use lib_parse::canonical::{CanonicalFault, CanonicalReading};
use test_kit::seplos::{BatteryState, Behavior, SeplosSim};

/// Cucumber `World` — all scenario state is `Option`/`Vec`, populated lazily by whichever steps a
/// given scenario actually runs (most scenarios only touch a slice of this).
#[derive(cucumber::World, Debug, Default)]
pub struct SpecWorld {
    /// The simulated battery's source-of-truth state (set by the `SeplosSim`-backed background
    /// steps; `None` for scenarios that construct a `CanonicalReading`/`CanonicalFault` directly).
    state: Option<BatteryState>,
    /// The pure Seplos responder wrapping `state` — reused verbatim from `test-kit`.
    sim: Option<SeplosSim>,
    /// `Behavior` has no `Default` impl by design (there is no "default" bus fault), so it lives
    /// in an `Option`, not a bare field.
    behavior: Option<Behavior>,
    /// DIP address of the device most recently polled / whose frame is being decoded.
    dip: u8,
    /// The raw on-wire response bytes most recently produced by the simulator (or the verbatim
    /// `LIVE_FRAME` fixture).
    raw_frame: Vec<u8>,
    /// The canonical telemetry reading most recently decoded.
    reading: Option<CanonicalReading>,
    /// The canonical fault flags most recently decoded or constructed.
    fault: Option<CanonicalFault>,
    /// The `DcMeasurementUpdate` most recently mapped from `reading`.
    dc: Option<DcMeasurementUpdate>,
    /// The `PowerHardwareUpdate` most recently mapped from `reading`.
    hw: Option<PowerHardwareUpdate>,
    /// The `ActiveDiagnosticState` most recently mapped from `fault`.
    diag: Option<ActiveDiagnosticState>,
    /// The `CanonicalReading` recovered by decoding `dc`/`hw` back through `FromTms`, for
    /// read/decode-direction round-trip scenarios.
    round_tripped: Option<CanonicalReading>,
    /// The `Display` text of the most recent decode error, if the last decode/parse failed.
    last_error: Option<String>,
    /// Scenario state for the resilience feature area (empty scaffold; filled by a later agent).
    pub resilience: steps::resilience::ResilienceState,
    /// Scenario state for the security feature area (empty scaffold; filled by a later agent).
    pub security: steps::security::SecurityState,
    /// Scenario state for the mqtt feature area (empty scaffold; filled by a later agent).
    pub mqtt: steps::mqtt::MqttState,
    /// Scenario state for the tms feature area (empty scaffold; filled by a later agent).
    pub tms: steps::tms::TmsState,
    /// Scenario state for the rtps_spec feature area (XCDR2/RTPS wire-encoding conformance, #325).
    pub rtps: steps::rtps::RtpsState,
    /// Scenario state for the simulation feature area (#226 stage W2: runtime battery-state spec
    /// driven positive/negative/edge simulation).
    pub simulation: steps::simulation::SimulationState,
    /// Scenario state for the multi-port feature area (#238 fan-out, #242 multi-port).
    pub bus: steps::bus::BusState,
    /// Scenario state for the RS-232 Voltronic inverter transform feature area (D10, decision 0001).
    pub inverter: steps::inverter::InverterState,
    /// Scenario state for the legacy AWS Greengrass backwards-compat feature area (Scope D,
    /// closes #52-#54).
    pub legacy: steps::legacy::LegacyState,
    /// Scenario state for the tms_breadth feature area (Heartbeat, DeviceInfo family,
    /// StorageUpdate, Request/Reply — #240 #244 #246).
    pub tms_breadth: steps::tms_breadth::TmsBreadthState,
    /// Scenario state for the Meshtastic mesh-radio condensation feature area (#239).
    pub mesh: steps::mesh::MeshState,
}

/// Deterministic device id used by every scenario (mirrors `tms_live_frame.rs`/`tms_transform.rs`).
pub(crate) fn dev() -> HString<32> {
    let mut s = HString::new();
    s.push_str("METEOR-001")
        .expect("METEOR-001 fits String<32>");
    s
}

/// Deterministic timestamp used by every scenario (mirrors `tms_live_frame.rs`).
pub(crate) fn now() -> ClockMonotonic {
    ClockMonotonic {
        seconds: 42,
        nanoseconds: 7,
    }
}

/// Float tolerance for every "approximately" assertion in the suite — the tighter of the two
/// source tests' tolerances (`tms_transform.rs` uses 0.01, `tms_live_frame.rs` uses 0.005), so
/// every scenario here is held to at least the LIVE_FRAME test's bar.
pub(crate) const TOL: f32 = 0.005;

/// Copied from `tms_live_frame.rs`/`tms_transform.rs`'s own `approx()` helper.
pub(crate) fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < TOL
}

/// Cucumber's `Cucumber`/`Runner`/`Writer` future chain nests deeply enough to overflow the
/// default 1 MiB Windows main-thread stack (a known cucumber-rs issue on Windows, unrelated to
/// this suite's own step logic) — so the runtime runs on a dedicated thread with a larger stack
/// instead of via `#[tokio::main]` on `main` directly.
///
/// `.fail_on_skipped()` (#276): a step whose Gherkin text doesn't match any compiled
/// `#[given/when/then]` expression is reported *skipped*, not *failed*, and by default the
/// process still exits 0 — silently blinding the acceptance gate to whole scenarios (this bit us
/// with `multi_port.feature`'s `platform(s)` vs. the step def's Cucumber-Expression optional-text
/// `(s)` group, which matched neither `platform` nor `platforms` literally). This turns any
/// skipped step into a hard failure for every feature/scenario run here (no `@allow.skipped`
/// tags are in use, so there's no intentional-skip escape hatch to preserve).
fn main() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Runtime::new()
                .expect("build tokio runtime")
                .block_on(
                    SpecWorld::cucumber()
                        .fail_on_skipped()
                        .run_and_exit("tests/features"),
                );
        })
        .expect("spawn cucumber runner thread")
        .join()
        .expect("cucumber runner thread panicked");
}
