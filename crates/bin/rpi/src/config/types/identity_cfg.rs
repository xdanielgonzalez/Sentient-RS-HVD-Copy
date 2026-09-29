use lib_mapping::schema::DeviceRole;

use super::protocol_cfg::ProtocolCfg;

/// One serial bus's derived TMS identity (D13). Each configured `[[bus]]` maps to exactly one
/// `IdentityCfg`, derived by [`crate::config::Config::identities`]: a unit with both a battery
/// (RS-485/Seplos) and an inverter (RS-232/Voltronic) leg genuinely runs TWO identities/roles
/// concurrently, one per bus (decision 0007).
///
/// - `device_id` — already truncated to the `Identity` 32-char bound; `bus.device_id` when set,
///   else `{device.id}` for a Seplos bus or `{device.id}-inv` for a Voltronic bus. Duplicates
///   across buses are a config-load error (see [`crate::config::Config::identities`]).
/// - `role` — derived from `protocol`, NOT from `[device].role` (which is now accepted-but-ignored):
///   `Seplos` → [`DeviceRole::Storage`], `Voltronic` → [`DeviceRole::Conversion`].
/// - `protocol` — selects the poll loop and the legacy `device_type` ([`IdentityCfg::device_type`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityCfg {
    pub device_id: String,
    pub role: DeviceRole,
    pub protocol: ProtocolCfg,
}
