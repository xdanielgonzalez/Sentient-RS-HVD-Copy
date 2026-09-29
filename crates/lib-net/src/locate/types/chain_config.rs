use super::location_source::LocationSource;

/// Ordered fallback chain of location sources, most preferred first, plus every tier's settings
/// needed to actually build a provider for it (see `impl_chain.rs`'s `build_providers`) — the
/// legacy baseline's fixed priority (Manual > GpsNmea > ModemManager > IpGeo, "manual override
/// wins") is expressed as `order`'s default value, not a hardcoded walk, so a deployment can still
/// reorder or omit tiers.
///
/// NOTE for the follow-up integration work (`crates/bin/rpi/src/config/impl_location_config.rs`'s
/// `LocationConfig::to_chain`, out of this issue's (#241) file ownership — see that file's own
/// doc comment): this struct grew from its WS0 scaffold shape (`order` + `refresh_secs` only) to
/// carry every tier's settings, mirroring `crate::mesh::MeshConfig`/`lib_persist::JsonlPolicy`'s role
/// as the complete settings bundle their `run_*` functions take. `to_chain()`'s existing
/// `LocationChain { order, refresh_secs }` two-field literal will need the new fields below filled
/// in from `LocationConfig`'s new `manual_lat`/`manual_lon`/`gps_port`/`ip_geo_url`/`*_enabled`
/// fields (added this issue) before location tagging is live end-to-end; `run_refresher`'s own
/// signature is unchanged, so that part of `runtime::supervise`'s wiring keeps working as-is.
#[derive(Debug, Clone, Default)]
pub struct LocationChain {
    /// Fallback attempt order, most preferred first.
    pub order: Vec<LocationSource>,
    /// Cadence `run_refresher` re-attempts the chain at (also passed as `run_refresher`'s own
    /// `refresh_secs` argument — kept here too so a `LocationChain` is self-describing).
    pub refresh_secs: u64,
    /// Manual-override fix `(lat, lon)`. The `Manual` tier is built only when this is `Some`.
    pub manual: Option<(f64, f64)>,
    /// GPS-NMEA serial device path. The `GpsNmea` tier is built only when this is `Some` AND
    /// `gps_enabled`.
    pub gps_port: Option<String>,
    /// IP-geolocation HTTP endpoint (`http://...` — see `impl_ip_geo.rs`'s doc comment on why
    /// HTTPS is deferred). The `IpGeo` tier is built only when this is `Some` AND
    /// `ip_geo_enabled`.
    pub ip_geo_url: Option<String>,
    /// Per-tier enable flags, independent of `order`'s membership (a tier can be listed in
    /// `order` but administratively disabled without editing the order list).
    pub gps_enabled: bool,
    pub modem_enabled: bool,
    pub ip_geo_enabled: bool,
}
