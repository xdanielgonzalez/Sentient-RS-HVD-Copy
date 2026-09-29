use super::capture_handle::CaptureHandle;
use super::capture_serial::CaptureSerial;
use super::cycle_pulse::CyclePulse;
use super::emit_cadence::EmitCadence;
use super::preflight_inputs::PreflightInputs;
use super::sink_set::SinkSet;
use lib_io::host::{DirectionMode, SerialKind};
use lib_io::transport::SerialParams;
use lib_mqtt::broker::BrokerPublisher;
use std::time::{Duration, Instant};

/// The RPi platform for ONE configured serial bus: owns this bus's publisher handle onto the
/// (externally owned) embedded broker, its configured secondary-publish taps, the serial
/// transport, and a monotonic clock origin. Retains the serial open parameters so the transport
/// can be reopened after a device loss ([`spec_app::Platform::reconnect`]).
///
/// Multi-port (#238, #242): the embedded broker itself is hoisted OUT of `HostPlatform` and owned
/// by `bin-rpi::runtime::supervise`, which mints one [`BrokerPublisher`] per configured bus via
/// `LocalBroker::publisher()` (cheap — the publisher shares the broker's `LinkTx` via `Arc`) and
/// runs one `impl_app::run(HostPlatform, RunConfig)` task per bus. Fan-out to secondary publish
/// targets (disk/Greengrass/mesh) is per-target isolated via `taps`, constructed once per process
/// (shared handles cloned into every bus that needs them) and threaded through this bus's
/// `HostPlatform` — see [`crate::TapEvent`].
pub struct HostPlatform {
    pub(in super::super) publisher: BrokerPublisher,
    pub(in super::super) taps: SinkSet,
    pub(in super::super) serial: CaptureSerial,
    pub(in super::super) start: Instant,
    /// This bus's label (config `[[bus]] name`, or `"default"` for the legacy single-`[serial]`
    /// config) — threaded into every [`crate::TapEvent`] this platform offers, and used in
    /// preflight/log context.
    pub(in super::super) bus: String,
    /// RS-485 driver-enable (DE/RE) mechanism this bus's port was opened with (see
    /// [`DirectionMode`]'s doc comment for the three-mode seam). Retained (not just consumed at
    /// `build_port` time) so [`spec_app::Platform::reconnect`] reopens with the SAME mechanism rather than
    /// silently falling back to the default on every reconnect.
    pub(in super::super) direction: DirectionMode,
    /// Liveness pulse this platform's `on_cycle` pets every completed poll cycle; aggregated by
    /// `bin-rpi::runtime::supervise`'s [`crate::WatchdogAgg`].
    pub(in super::super) cycle_pulse: CyclePulse,
    // Serial open parameters, kept for reconnect.
    pub(in super::super) port: String,
    pub(in super::super) kind: SerialKind,
    pub(in super::super) params: SerialParams,
    pub(in super::super) read_timeout_ms: u32,
    /// Raw pre-parse wire-capture handle (decision 0001's open risk), re-threaded into every
    /// reopened [`CaptureSerial`] on [`spec_app::Platform::reconnect`] so a reconnected port keeps recording
    /// to the SAME writer/channel rather than silently going passthrough. `None` when `[capture]`
    /// isn't configured — the fast-path passthrough case (see `CaptureSerial`'s doc comment).
    pub(in super::super) capture: Option<CaptureHandle>,
    // Non-secret inputs the preflight checks inspect, gathered by the caller from `Config`.
    pub(in super::super) preflight_inputs: PreflightInputs,
    /// Output-cadence state enforcing the TMS publication-timing profiles (`PowerHardwareUpdate`/
    /// `StorageUpdate` 10 s rate limit; `ActiveDiagnosticState` At-Start/On-Change) — see
    /// [`EmitCadence`]. Per-bus, so each bus rate-limits independently.
    pub(in super::super) cadence: EmitCadence,
    /// Minimum interval between `PowerHardwareUpdate`/`StorageUpdate` publishes, sourced from the
    /// process's resolved `TimingProfile` at [`HostPlatform::build_port`] time (production: 10 s,
    /// matching [`EmitCadence::PERIODIC_10S`]). TEST-ONLY compression (debug builds, via
    /// `SENTIENT_TEST_FAST_TIMERS`) shortens this so e2e tests don't wait through the full spec
    /// window — see `crate::TimingProfile` for the full contract.
    pub(in super::super) rate_limit_period: Duration,
}
