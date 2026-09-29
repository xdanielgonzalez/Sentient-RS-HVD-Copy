use serde::Deserialize;

/// RS-485 driver-enable (DE/RE) mechanism — the field-hardware-risk decision the gap analysis
/// flags ("RS-485 direction control mechanism"). Mirrors `lib_io::host::DirectionMode`'s
/// three-way split one level up (this is the plain, field-less config selector; `lib-io`'s own
/// `DirectionMode` carries the actual settle-time/GPIO-line fields — see `SerialConfig`, which
/// pairs this enum with those). Mirrors `parity_cfg.rs`/`serial_kind_cfg.rs`'s shape.
///
/// Two surfaces reuse this same type, both now live (wired via [`crate::config::SerialConfig::direction_mode`]):
///  * the top-level `[direction]` section (`Config::direction: Option<DirectionCfg>`) — the
///    process-wide fallback, used when a bus/port doesn't set its own `direction`;
///  * `[serial]`/`[[bus]].serial`'s `direction` field (`SerialConfig::direction:
///    Option<DirectionCfg>`, #245) — the concrete, per-port override; see `SerialConfig`'s doc
///    comment for the precedence (`bin-rpi::runtime::supervise` resolves it once per bus via
///    `SerialConfig::direction_mode(cfg.direction)`, then passes the resulting
///    `lib_io::host::DirectionMode` into `HostPlatform::build_port`).
///
/// `Tiocsrs485` accepts BOTH `"tiocsrs485"` (its primary/serialized spelling, kept for the
/// pre-existing top-level `[direction]` surface) and `"kernel"` (the spelling this workstream's
/// per-port `[serial]`/`[[bus]].serial` config docs use, and the more operator-friendly of the
/// two) via `#[serde(alias = ...)]` — one Rust variant, two accepted TOML strings.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DirectionCfg {
    #[default]
    Rts,
    #[serde(alias = "kernel")]
    Tiocsrs485,
    Gpio,
}
