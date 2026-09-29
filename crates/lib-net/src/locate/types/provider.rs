use super::location_fix::LocationFix;

/// A single tier in the location fallback chain (GPS-NMEA, ModemManager, IP-geolocation, manual
/// override). Each concrete provider implements this against its own transport — see
/// `impl_gps_nmea.rs` / `impl_modem_manager.rs` / `impl_ip_geo.rs` / `impl_manual.rs`.
#[allow(async_fn_in_trait)]
pub trait LocationProvider {
    /// Attempt to acquire a fresh fix. `Err` carries a short human-readable reason so
    /// `crate::locate::run_refresher` can log why this tier was skipped before falling through to the
    /// next one in the chain.
    async fn locate(&mut self) -> Result<LocationFix, &'static str>;
}
