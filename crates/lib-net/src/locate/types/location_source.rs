/// Which provider produced a [`super::location_fix::LocationFix`]. The
/// [`super::chain_config::LocationChain`]'s own `order: Vec<LocationSource>` field carries the
/// actual preferred fallback order — this declaration order is not itself a priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationSource {
    Manual,
    GpsNmea,
    ModemManager,
    IpGeo,
}
