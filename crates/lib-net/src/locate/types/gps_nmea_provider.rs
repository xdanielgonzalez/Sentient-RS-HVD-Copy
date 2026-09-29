/// The GPS-NMEA tier: reads `$--GGA`/`$--RMC` sentences off a serial-attached GPS/GNSS receiver
/// (tokio-serial + a minimal in-crate parser — no `nmea` dependency; see `impl_gps_nmea.rs`).
/// Opens a fresh connection on every `locate()` call rather than holding one across ticks, so a
/// wedged/unplugged receiver can't leave the tier stuck on a dead handle.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GpsNmeaProvider {
    pub port: String,
    pub baud: u32,
    /// Overall budget (serial open + read-until-valid-fix) for one `locate()` call.
    pub read_timeout_ms: u64,
}
