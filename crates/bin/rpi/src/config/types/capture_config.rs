use serde::Deserialize;

/// Raw pre-parse wire capture-to-disk (decision 0001's open risk: "this environment had no real
/// Voltronic/QPIGS wire captures... must be verified against real captures... before an inverter
/// deployment is trusted"). Every raw Voltronic/Seplos request+response exchange, hex-encoded,
/// tailed to a rotating jsonl log for field diagnostics — independent of `[disk]`'s already-
/// parsed/rendered TMS telemetry tail (`DiskConfig`). See `lib_io::persist::RawCaptureRecord` /
/// `lib_io::persist::run_capture_writer` for the writer this maps to (`CaptureConfig::to_policy`, in
/// `impl_capture_config.rs`).
///
/// Wired end to end (Wave 4, gap-closure #237/#268): `runtime.rs`'s `supervise` spawns
/// `run_capture_writer` once per process when `[capture]` is present and threads a
/// `bin_rpi_platform::CaptureHandle` into every configured bus's
/// `bin_rpi_platform::HostPlatform::build_port`, which wraps the bus's `HostSerial` in a
/// `bin_rpi_platform::CaptureSerial` decorator — presence of `[capture]` is the opt-in signal
/// (mirrors the `[disk]`/`[meshtastic]`/`[greengrass]`/`[location]` opt-in-by-presence pattern);
/// absent, every bus's `CaptureSerial` is a zero-overhead passthrough.
///
/// Unlike `DiskConfig`, `dir` has NO default: a raw wire-capture tool is an explicit opt-in field
/// diagnostic, not a Day-1-enabled feature, so there is no sensible universal default directory —
/// a deployment that wants this must say where.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureConfig {
    /// Directory the rotating raw-capture jsonl files live in. Required (no default) — presence of
    /// `[capture]` at all is the opt-in signal, but this field must still be explicit.
    pub dir: String,
    /// Roll to a new file once the active file reaches this many bytes. Must be non-zero — see
    /// [`CaptureConfig::validate`] (same degenerate-rotation-churn hazard as
    /// `DiskConfig::max_file_bytes`).
    #[serde(default = "default_capture_max_file_bytes")]
    pub max_file_bytes: u64,
    /// Total on-disk footprint budget (bytes) across the active file plus every retained rotated
    /// segment — converted to `JsonlPolicy::max_files` in `to_policy` (see
    /// `impl_capture_config.rs`), unlike `DiskConfig` which takes a file COUNT directly. A byte
    /// budget is the more useful knob here: raw captures are a diagnostics tool run for a bounded
    /// field-collection window, not an indefinitely-retained telemetry tail.
    #[serde(default = "default_capture_max_total_bytes")]
    pub max_total_bytes: u64,
    /// Bounded channel depth between the capturing `CaptureSerial` transport decorator and the
    /// writer task. `0` is defensively clamped to `1` at the channel-construction call site
    /// (`runtime.rs`'s `supervise`), so it is left unbounded here rather than duplicating that
    /// floor.
    #[serde(default = "default_capture_queue_depth")]
    pub queue_depth: usize,
}

fn default_capture_max_file_bytes() -> u64 {
    10 * 1024 * 1024
}
fn default_capture_max_total_bytes() -> u64 {
    100 * 1024 * 1024
}
fn default_capture_queue_depth() -> usize {
    256
}
