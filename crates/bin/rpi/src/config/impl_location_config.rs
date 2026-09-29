use super::LocationConfig;
use lib_net::locate::{LocationChain, LocationSource};

impl LocationConfig {
    /// Map to the `lib-locate` fallback chain `lib_net::locate::run_refresher` walks. Unrecognized
    /// `order` names are skipped with a warning rather than rejected — this mapping (like the
    /// rest of `[location]`, see `LocationConfig`'s doc comment) is PROVISIONAL (gap-closure
    /// #237); a stricter reject-at-load-time posture can land once the config surface is IDL/
    /// reference-verified.
    ///
    /// Threads every per-tier setting (`manual_lat`/`manual_lon`/`gps_port`/`ip_geo_url`/the
    /// `*_enabled` flags) into `LocationChain`'s matching fields — see that type's doc comment
    /// for why this mapping exists (closes the previously-documented `#241`/`#237` integration
    /// gap between the `locate` and `runtime` work streams). The `manual` tier is active only
    /// when BOTH `manual_lat` and `manual_lon` are present, matching `LocationConfig`'s own doc
    /// comment.
    pub fn to_chain(&self) -> LocationChain {
        let order = self
            .order
            .iter()
            .filter_map(|name| match name.as_str() {
                "manual" => Some(LocationSource::Manual),
                "gps-nmea" => Some(LocationSource::GpsNmea),
                "modem-manager" => Some(LocationSource::ModemManager),
                "ip-geo" => Some(LocationSource::IpGeo),
                other => {
                    log::warn!("location.order: unrecognized provider {other:?}; skipped");
                    None
                }
            })
            .collect();
        LocationChain {
            order,
            refresh_secs: self.refresh_secs,
            manual: self.manual_lat.zip(self.manual_lon),
            gps_port: self.gps_port.clone(),
            // `build_one` already gates the IP-geo tier on `ip_geo_enabled` separately (see
            // `LocationChain::build_providers`), so the URL itself is passed through unconditionally.
            ip_geo_url: Some(self.ip_geo_url.clone()),
            gps_enabled: self.gps_enabled,
            modem_enabled: self.modem_enabled,
            ip_geo_enabled: self.ip_geo_enabled,
        }
    }
}
