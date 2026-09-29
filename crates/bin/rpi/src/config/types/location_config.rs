use serde::Deserialize;

/// Multi-tier location tagging — see `docs/gap-analysis-meteor-parity.md`'s "Multi-tier location
/// service" finding (the legacy AWS pipeline's location service, not yet re-homed). Mirrors
/// `lib_net::locate::LocationChain`'s fields (order as the lowercase `LocationSource` names, e.g.
/// `["manual", "gps-nmea", "modem-manager", "ip-geo"]` — this crate's `LocationConfig::to_chain`
/// already maps that part).
///
/// This struct grew (issue #241, gap-closure #237) from its WS0 scaffold shape (`order` +
/// `refresh_secs` only) to also carry every tier's settings, once `lib-locate`'s own
/// `LocationChain` grew the matching fields (see that crate's `types/chain_config.rs` doc
/// comment). **`impl_location_config.rs`'s `to_chain()` is NOT owned by this issue** and still
/// only maps `order`/`refresh_secs` as of this change — wiring `manual_lat`/`manual_lon`/
/// `gps_port`/`ip_geo_url`/the `*_enabled` flags below into `LocationChain`'s new fields (and
/// therefore into `runtime::supervise`'s live wiring) is the deferred follow-up work this file's
/// original doc comment already flagged.
///
/// PROVISIONAL (gap-closure #237, issue #241): no reference prototype was available in this
/// sandbox (`/home/user/meteor-adapter` absent) to confirm the legacy pipeline's real TOML key
/// names/shape, so this shape — flat `manual_lat`/`manual_lon` keys rather than a
/// `[location.manual]` sub-table, and the specific per-tier `*_enabled` names — is a best-effort,
/// self-consistent design, not a verified port. See
/// `docs/gap-analysis-meteor-parity.md`'s "Multi-tier location service" finding.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocationConfig {
    /// Fallback provider order, most preferred first. Defaults to the baseline priority chain:
    /// Manual > GpsNmea > ModemManager > IpGeo ("manual override wins").
    #[serde(default = "default_location_order")]
    pub order: Vec<String>,
    /// How often (seconds) to refresh the location fix. Left unbounded: `lib_net::locate::run_refresher`
    /// already floors `0` to `1` (`refresh_secs.max(1)`) before building its `tokio::time::interval`,
    /// so a `0` here degrades to "refresh every second" rather than panicking or doing nothing —
    /// a legitimate (if aggressive) choice, not a hazard worth failing startup over.
    #[serde(default = "default_location_refresh_secs")]
    pub refresh_secs: u64,
    /// Manual-override fix's latitude, degrees. The `manual` tier is active only when BOTH
    /// `manual_lat` and `manual_lon` are present.
    #[serde(default)]
    pub manual_lat: Option<f64>,
    /// Manual-override fix's longitude, degrees — see `manual_lat`.
    #[serde(default)]
    pub manual_lon: Option<f64>,
    /// Serial device path for the GPS-NMEA tier (e.g. `/dev/ttyACM0`). The tier is active only
    /// when this is set AND `gps_enabled` is true.
    #[serde(default)]
    pub gps_port: Option<String>,
    /// Plain-HTTP (not HTTPS — a second TLS stack would trip the workspace's duplicate-rustls-
    /// version ban; see `lib_net::locate`'s crate docs) IP-geolocation endpoint for the IP-geo tier.
    #[serde(default = "default_ip_geo_url")]
    pub ip_geo_url: String,
    /// GPS-NMEA tier enable, independent of `order`'s membership (a tier can be listed in `order`
    /// but administratively disabled without editing the order list). Defaults on — harmless
    /// without `gps_port` also set, since the tier stays inert either way.
    #[serde(default = "default_true")]
    pub gps_enabled: bool,
    /// ModemManager tier enable — see `gps_enabled`. Defaults on — harmless without ModemManager
    /// installed, since an absent `mmcli` binary self-disables the tier per-tick.
    #[serde(default = "default_true")]
    pub modem_enabled: bool,
    /// IP-geolocation tier enable — see `gps_enabled`. Defaults OFF (unlike the other two tiers):
    /// this is the one tier that reaches a third-party service over the network by default
    /// behavior, which should be an explicit opt-in, not a silent default.
    #[serde(default)]
    pub ip_geo_enabled: bool,
}

fn default_location_order() -> Vec<String> {
    vec![
        "manual".to_string(),
        "gps-nmea".to_string(),
        "modem-manager".to_string(),
        "ip-geo".to_string(),
    ]
}

fn default_location_refresh_secs() -> u64 {
    60
}

fn default_ip_geo_url() -> String {
    "http://ip-api.com/json".to_string()
}

fn default_true() -> bool {
    true
}
