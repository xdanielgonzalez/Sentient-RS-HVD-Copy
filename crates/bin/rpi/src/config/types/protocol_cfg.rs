use serde::Deserialize;

/// Which vendor protocol a `[[bus]]` speaks (D9/D13). Selects the poll loop
/// (`impl_app::run` vs `impl_app::run_inverter`) and — via [`crate::config::IdentityCfg`] — the
/// bus's TMS role (`Seplos` → `ROLE_STORAGE`, `Voltronic` → `ROLE_CONVERSION`; decision 0007) and
/// its legacy `device_type` (`"battery"` / `"inverter"`).
///
/// Defaults to [`ProtocolCfg::Seplos`] so a pre-D13 `[[bus]]`/`[serial]` config (no `protocol` key)
/// keeps loading unchanged as a battery/RS-485 bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProtocolCfg {
    /// Seplos v2 BMS (RS-485) — the battery leg.
    #[default]
    Seplos,
    /// Voltronic/MPP-Solar PI30 inverter (RS-232) — the inverter leg (decision 0001).
    Voltronic,
}
