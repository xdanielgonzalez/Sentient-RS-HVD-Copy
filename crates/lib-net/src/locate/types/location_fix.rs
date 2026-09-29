use super::location_source::LocationSource;

/// A single location tag for telemetry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocationFix {
    pub lat: f64,
    pub lon: f64,
    pub alt_m: f64,
    pub source: LocationSource,
    /// Wall-clock milliseconds at which this fix was acquired.
    pub ts_ms: u64,
}
