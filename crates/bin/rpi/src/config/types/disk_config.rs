use serde::Deserialize;

/// Disk / JSON-lines telemetry logging (Scope A, #55/#56/#57) — see
/// `docs/gap-analysis-meteor-parity.md`'s "Disk / JSON-lines telemetry logging" finding, and
/// `docs/demo-disk-logging.md` for the runbook. Mirrors `lib_io::persist::JsonlPolicy`'s fields
/// one-for-one; presence of `[disk]` in the config (i.e. `Config::disk` being `Some`) is what
/// enables the tap — see `DiskConfig::to_policy` (`impl_disk_config.rs`) for the mapping and
/// `runtime::supervise` for where the writer task is spawned and wired into every bus's
/// `SinkSet` (mirrors the `[meshtastic]`/`[greengrass]`/`[location]` opt-in-by-presence pattern).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiskConfig {
    /// Directory the rotating log files live in.
    #[serde(default = "default_disk_dir")]
    pub dir: String,
    /// Filename prefix (e.g. `tms` -> `tms.jsonl` active, rotated to `tms.1.jsonl`, `tms.2.jsonl`, ..).
    #[serde(default = "default_disk_file_prefix")]
    pub file_prefix: String,
    /// Roll to a new file once the active file reaches this many bytes. Must be non-zero — see
    /// [`DiskConfig::validate`].
    #[serde(default = "default_disk_max_file_bytes")]
    pub max_file_bytes: u64,
    /// Maximum number of rotated files retained. `0` is a legitimate "retain no history, active
    /// file only" choice — left unbounded, see the umbrella task's judgement notes.
    #[serde(default = "default_disk_max_files")]
    pub max_files: usize,
    /// `fsync` the active file every N lines written. `0` deliberately disables periodic fsync
    /// (relying only on the shutdown-drain flush) — an intentional, already-documented switch, not
    /// bounded away.
    #[serde(default = "default_disk_fsync_every_lines")]
    pub fsync_every_lines: usize,
    /// Bounded channel depth between the poll loop and the writer task. `0` is defensively
    /// clamped to `1` at the channel-construction call site (`runtime.rs`'s `supervise`), so it is
    /// left unbounded here rather than duplicating that floor.
    #[serde(default = "default_disk_queue_depth")]
    pub queue_depth: usize,
    /// How often (ms) to re-probe a disk-full condition before resuming writes. Left unbounded: a
    /// `0` retries every write while degraded (busier, not unsafe — bounded by the poll cadence,
    /// never a tight spin loop) rather than something no operator could ever want.
    #[serde(default = "default_disk_full_probe_ms")]
    pub full_probe_ms: u64,
}

fn default_disk_dir() -> String {
    "/var/lib/sentient/telemetry".into()
}
fn default_disk_file_prefix() -> String {
    "tms".into()
}
fn default_disk_max_file_bytes() -> u64 {
    10 * 1024 * 1024
}
fn default_disk_max_files() -> usize {
    10
}
fn default_disk_fsync_every_lines() -> usize {
    100
}
fn default_disk_queue_depth() -> usize {
    256
}
fn default_disk_full_probe_ms() -> u64 {
    5000
}
