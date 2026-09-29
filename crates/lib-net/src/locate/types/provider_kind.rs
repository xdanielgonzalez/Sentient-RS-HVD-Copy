use super::gps_nmea_provider::GpsNmeaProvider;
use super::ip_geo_provider::IpGeoProvider;
use super::manual_provider::ManualProvider;
use super::modem_manager_provider::ModemManagerProvider;

/// Static stand-in for `dyn `[`super::provider::LocationProvider`]: that trait's `locate` is an
/// `async fn` in its definition, which is not `dyn`-safe, so the chain walk (`impl_chain.rs`)
/// dispatches over this closed enum (one variant per concrete provider) instead of a trait object.
#[derive(Debug)]
pub(crate) enum ProviderKind {
    Manual(ManualProvider),
    GpsNmea(GpsNmeaProvider),
    ModemManager(ModemManagerProvider),
    IpGeo(IpGeoProvider),
}
