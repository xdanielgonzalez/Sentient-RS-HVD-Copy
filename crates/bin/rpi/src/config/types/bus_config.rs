use super::poll_config::PollConfig;
use super::protocol_cfg::ProtocolCfg;
use super::serial_config::SerialConfig;
use serde::Deserialize;

/// One entry of the `[[bus]]` array-of-tables — the N-port `InterfaceManager`-equivalent (see
/// `docs/gap-analysis-meteor-parity.md`'s "Multi-port serial (InterfaceManager)" finding).
/// Same shape as the legacy single `[serial]`/`[poll]` pair, plus a `name` that labels this bus
/// in logs, taps (`TapEvent::bus`), and the watchdog aggregator.
///
/// `[[bus]]` and the legacy top-level `[serial]` are mutually exclusive — see
/// `Config::buses`'s doc comment.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusConfig {
    /// This bus's label (e.g. `"pack-a"`). Free text; used only for logs/taps/watchdog naming.
    pub name: String,
    pub serial: SerialConfig,
    #[serde(default)]
    pub poll: PollConfig,
    /// Vendor protocol this bus speaks (D9/D13). Selects the poll loop (Seplos BMS vs Voltronic
    /// inverter) and, via [`crate::config::IdentityCfg`], the bus's TMS role + legacy `device_type`.
    /// Defaults to [`ProtocolCfg::Seplos`] so a pre-D13 `[[bus]]` (no `protocol` key) is unchanged.
    #[serde(default)]
    pub protocol: ProtocolCfg,
    /// Explicit TMS identity for this bus (D13), overriding the derived default (`{device.id}` for a
    /// Seplos bus, `{device.id}-inv` for a Voltronic bus). Truncated to the `Identity` 32-char bound;
    /// duplicates across buses are a config-load error (see [`crate::config::Config::identities`]).
    #[serde(default)]
    pub device_id: Option<String>,
}
