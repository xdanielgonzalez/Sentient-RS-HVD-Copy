use serde::Deserialize;

/// `[device]` — this device's TMS identity, announced via the `Heartbeat`/`DeviceInfo` topics
/// (#240/#244) and used as the `@key` on every published TMS payload.
///
/// PROVISIONAL: `role`/`manufacturer`/`model`/`serial_number`/`firmware_version` are new surface
/// added for `DeviceInfo` (gap-closure #237, TMS breadth #240/#244/#246) — no reference
/// prototype/IDL was available in this environment to verify field names/lengths against (see
/// CLAUDE.md's reference-prototype policy); each is truncated to `String32`'s 32-char bound (see
/// `lib_tms::String32`) when mapped into `DeviceInfo`/`ProductInfo`. All default to empty (or
/// `"STOR"` for `role`) so an existing pre-#237 `[device]` section with only `id` set keeps loading
/// unchanged.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceConfig {
    /// Device identity published on the TMS topics (`Identity`, ≤32 chars).
    pub id: String,
    /// TMS device role. **Accepted-but-IGNORED since D13** (decision 0007): the role is now derived
    /// PER BUS from the bus protocol (`Seplos` → `ROLE_STORAGE`, `Voltronic` → `ROLE_CONVERSION`) in
    /// [`crate::config::Config::identities`], not from this single global string — a unit with both
    /// legs runs two roles concurrently, one per bus. The field is retained (default `"STOR"`) so a
    /// pre-D13 `[device]` section keeps loading unchanged; it no longer influences `DeviceInfo`.
    // Still deserialized (a real config key) but no longer READ by production code — the role is
    // derived per bus now (see `Config::identities`). Kept for backward-compat config parsing.
    #[allow(dead_code)]
    #[serde(default = "default_role")]
    pub role: String,
    /// `ProductInfo.manufacturer`.
    #[serde(default)]
    pub manufacturer: String,
    /// `ProductInfo.model`.
    #[serde(default)]
    pub model: String,
    /// `ProductInfo.serial_number`.
    #[serde(default)]
    pub serial_number: String,
    /// `ProductInfo.firmware_version`.
    #[serde(default)]
    pub firmware_version: String,
}

fn default_role() -> String {
    "STOR".to_string()
}
