use super::prelude::*;

/// Serial baud rate for the GPS-NMEA tier. Not exposed as a config knob (`docs/gap-analysis-
/// meteor-parity.md`'s "Multi-tier location service" finding lists only `gps_port`); 9600 is the
/// common default for modern GPS/GNSS modules (NMEA-0183's own historical default is 4800, but
/// most current hardware ships at 9600 or auto-bauds).
const GPS_NMEA_DEFAULT_BAUD: u32 = 9_600;
/// Per-`locate()` budget for the GPS-NMEA tier (serial open + read-until-fix).
const GPS_NMEA_DEFAULT_TIMEOUT_MS: u64 = 3_000;
const MODEM_MANAGER_DEFAULT_BINARY: &str = "mmcli";
const MODEM_MANAGER_DEFAULT_TIMEOUT_MS: u64 = 5_000;
const IP_GEO_DEFAULT_TIMEOUT_MS: u64 = 5_000;

impl LocationChain {
    /// Build this chain's provider list, following `order`'s declared sequence and skipping any
    /// tier whose necessary settings are absent or explicitly disabled — the "manual override
    /// wins" behavior falls straight out of `order` defaulting to Manual first (see
    /// `LocationChain`'s doc comment) plus [`attempt_once`] trying tiers in order and stopping at
    /// the first success.
    pub(crate) fn build_providers(&self) -> Vec<(LocationSource, ProviderKind)> {
        self.order
            .iter()
            .filter_map(|&source| self.build_one(source).map(|p| (source, p)))
            .collect()
    }

    fn build_one(&self, source: LocationSource) -> Option<ProviderKind> {
        match source {
            LocationSource::Manual => self
                .manual
                .map(|(lat, lon)| ProviderKind::Manual(ManualProvider { lat, lon })),
            LocationSource::GpsNmea => {
                if !self.gps_enabled {
                    return None;
                }
                let port = self.gps_port.clone()?;
                Some(ProviderKind::GpsNmea(GpsNmeaProvider {
                    port,
                    baud: GPS_NMEA_DEFAULT_BAUD,
                    read_timeout_ms: GPS_NMEA_DEFAULT_TIMEOUT_MS,
                }))
            }
            LocationSource::ModemManager => {
                if !self.modem_enabled {
                    return None;
                }
                Some(ProviderKind::ModemManager(ModemManagerProvider {
                    binary: MODEM_MANAGER_DEFAULT_BINARY.to_string(),
                    timeout_ms: MODEM_MANAGER_DEFAULT_TIMEOUT_MS,
                }))
            }
            LocationSource::IpGeo => {
                if !self.ip_geo_enabled {
                    return None;
                }
                let url = self.ip_geo_url.clone()?;
                Some(ProviderKind::IpGeo(IpGeoProvider {
                    url,
                    timeout_ms: IP_GEO_DEFAULT_TIMEOUT_MS,
                }))
            }
        }
    }
}

/// Try every provider in `providers`' order, returning the first success. Failures are logged at
/// `debug` (an expected, routine per-tick occurrence — e.g. a modem with no satellite lock yet —
/// not a fault) and the walk falls through to the next tier.
pub(crate) async fn attempt_once(
    providers: &mut [(LocationSource, ProviderKind)],
) -> Option<LocationFix> {
    for (source, provider) in providers.iter_mut() {
        match provider.locate().await {
            Ok(fix) => return Some(fix),
            Err(reason) => log::debug!("lib-locate: {source:?} tier failed this tick: {reason}"),
        }
    }
    None
}

/// Pure per-tick resolution policy: `fresh` is this tick's outcome (already the first successful
/// provider in priority order, from [`attempt_once`], or `None` if every tier failed/is
/// unconfigured). Kept separate from the async provider I/O above so it is unit-testable without
/// spinning up serial ports / subprocesses / sockets.
///
/// Staleness: a previously-acquired fix is kept published for up to `stale_after_ms` after its own
/// timestamp even when a tick fails to refresh it (one flaky GPS cold-fix or a busy modem
/// shouldn't flap the published location to `None` and back); once that budget is exceeded with no
/// fresh fix, it expires to `None`.
pub(crate) fn resolve_tick(
    fresh: Option<LocationFix>,
    last: &mut Option<LocationFix>,
    now_ms: u64,
    stale_after_ms: u64,
) -> Option<LocationFix> {
    if let Some(fix) = fresh {
        *last = Some(fix);
        return Some(fix);
    }
    match *last {
        Some(fix) if now_ms.saturating_sub(fix.ts_ms) <= stale_after_ms => Some(fix),
        _ => {
            *last = None;
            None
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn fix(source: LocationSource, ts_ms: u64) -> LocationFix {
        LocationFix {
            lat: 1.0,
            lon: 2.0,
            alt_m: 0.0,
            source,
            ts_ms,
        }
    }

    // ---- build_providers: order + skip logic -------------------------------------------------

    #[test]
    fn build_providers_follows_order_and_includes_every_configured_tier() {
        let chain = LocationChain {
            order: vec![
                LocationSource::Manual,
                LocationSource::GpsNmea,
                LocationSource::ModemManager,
                LocationSource::IpGeo,
            ],
            refresh_secs: 60,
            manual: Some((1.0, 2.0)),
            gps_port: Some("/dev/ttyFAKE".to_string()),
            ip_geo_url: Some("http://example.invalid/json".to_string()),
            gps_enabled: true,
            modem_enabled: true,
            ip_geo_enabled: true,
        };
        let built = chain.build_providers();
        assert_eq!(built.len(), 4);
        assert_eq!(built[0].0, LocationSource::Manual);
        assert!(matches!(built[0].1, ProviderKind::Manual(_)));
        assert_eq!(built[1].0, LocationSource::GpsNmea);
        assert!(matches!(built[1].1, ProviderKind::GpsNmea(_)));
        assert_eq!(built[2].0, LocationSource::ModemManager);
        assert!(matches!(built[2].1, ProviderKind::ModemManager(_)));
        assert_eq!(built[3].0, LocationSource::IpGeo);
        assert!(matches!(built[3].1, ProviderKind::IpGeo(_)));
    }

    #[test]
    fn build_providers_skips_unconfigured_and_disabled_tiers() {
        let chain = LocationChain {
            order: vec![
                LocationSource::Manual,
                LocationSource::GpsNmea,
                LocationSource::ModemManager,
                LocationSource::IpGeo,
            ],
            refresh_secs: 60,
            manual: None,               // no manual fix configured
            gps_port: Some("x".into()), // configured but...
            gps_enabled: false,         // ...disabled
            ip_geo_url: None,           // no url configured
            modem_enabled: true,
            ip_geo_enabled: true,
        };
        let built = chain.build_providers();
        assert_eq!(built.len(), 1);
        assert_eq!(built[0].0, LocationSource::ModemManager);
    }

    #[test]
    fn build_providers_respects_a_reordered_chain() {
        let chain = LocationChain {
            order: vec![LocationSource::IpGeo, LocationSource::Manual],
            manual: Some((5.0, 6.0)),
            ip_geo_url: Some("http://example.invalid/json".to_string()),
            ip_geo_enabled: true,
            ..Default::default()
        };
        let built = chain.build_providers();
        assert_eq!(built.len(), 2);
        assert_eq!(built[0].0, LocationSource::IpGeo);
        assert_eq!(built[1].0, LocationSource::Manual);
    }

    // ---- priority walk: first success wins, in order ------------------------------------------

    #[tokio::test]
    async fn manual_wins_even_when_a_later_tier_would_also_succeed() {
        let chain = LocationChain {
            order: vec![LocationSource::Manual, LocationSource::IpGeo],
            manual: Some((10.0, 20.0)),
            // Points nowhere real, but priority means it's never even tried.
            ip_geo_url: Some("http://127.0.0.1:1/json".to_string()),
            ip_geo_enabled: true,
            ..Default::default()
        };
        let mut providers = chain.build_providers();
        let result = attempt_once(&mut providers).await;
        let got = result.expect("manual tier always succeeds");
        assert_eq!(got.lat, 10.0);
        assert_eq!(got.lon, 20.0);
        assert_eq!(got.source, LocationSource::Manual);
    }

    #[tokio::test]
    async fn falls_through_to_the_next_tier_when_the_first_fails() {
        // Manual not configured (skipped at build time); GpsNmea points at a nonexistent serial
        // device (fails at locate() time) so the walk must fall through to Manual-less last
        // resort — here just IpGeo pointed at a closed port, proving the *order* still governs
        // which failure surfaces (GpsNmea's, tried first) without the whole call panicking.
        let chain = LocationChain {
            order: vec![LocationSource::GpsNmea, LocationSource::Manual],
            gps_port: Some("/dev/definitely-not-a-real-port".to_string()),
            gps_enabled: true,
            manual: Some((7.0, 8.0)),
            ..Default::default()
        };
        let mut providers = chain.build_providers();
        let got = attempt_once(&mut providers)
            .await
            .expect("falls through to manual");
        assert_eq!(got.source, LocationSource::Manual);
        assert_eq!(got.lat, 7.0);
    }

    #[tokio::test]
    async fn empty_chain_yields_no_fix() {
        let mut providers: Vec<(LocationSource, ProviderKind)> = Vec::new();
        assert!(attempt_once(&mut providers).await.is_none());
    }

    // ---- resolve_tick: staleness policy --------------------------------------------------------

    #[test]
    fn a_fresh_fix_is_published_and_remembered() {
        let mut last = None;
        let f = fix(LocationSource::Manual, 1_000);
        let out = resolve_tick(Some(f), &mut last, 1_000, 5_000);
        assert_eq!(out, Some(f));
        assert_eq!(last, Some(f));
    }

    #[test]
    fn a_missed_tick_keeps_publishing_the_last_fix_within_the_staleness_budget() {
        let mut last = Some(fix(LocationSource::GpsNmea, 1_000));
        let out = resolve_tick(None, &mut last, 4_000, 5_000);
        assert_eq!(out, last);
    }

    #[test]
    fn a_missed_tick_expires_the_fix_once_the_staleness_budget_is_exceeded() {
        let mut last = Some(fix(LocationSource::GpsNmea, 1_000));
        let out = resolve_tick(None, &mut last, 10_000, 5_000);
        assert_eq!(out, None);
        assert_eq!(last, None);
    }

    #[test]
    fn no_prior_fix_and_a_missed_tick_stays_none() {
        let mut last = None;
        let out = resolve_tick(None, &mut last, 1_000, 5_000);
        assert_eq!(out, None);
    }
}
