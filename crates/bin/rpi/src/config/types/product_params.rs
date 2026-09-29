/// The shared `ProductInfo` fields injected into each per-identity heartbeat/DeviceInfo task
/// (D13) — one product per unit, the same for every bus's identity. Owned/cloneable so the
/// heartbeat tasks are handed their inputs directly rather than re-loading [`crate::config::Config`]
/// (the pre-D13 tasks re-read the TOML/env; injection removes that reload). Derived from
/// `[device]` via [`ProductParams::from_device`].
#[derive(Debug, Clone, Default)]
pub struct ProductParams {
    pub manufacturer: String,
    pub model: String,
    pub serial_number: String,
    pub firmware_version: String,
}
