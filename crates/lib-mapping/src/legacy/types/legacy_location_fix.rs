/// A location fix, expressed in the legacy module's own vocabulary — deliberately NOT
/// `lib_locate::LocationFix`. This crate (`lib-mapping`) must never depend on `lib-locate` (see
/// the `legacy` module's doc comment and `Cargo.toml`'s `legacy` feature comment): the caller
/// (`impl_app::host::GreengrassTap`, which already depends on both crates) maps its own
/// `lib_locate::LocationFix` into this shape before calling [`super::super::battery_payload`].
///
/// `source` is `&'static str` rather than `String`: every real source is one of a small, fixed
/// set of provider tags (`"manual"` / `"gps_nmea"` / `"modem_manager"` / `"ip_geo"`, mirroring
/// `lib_locate::LocationSource`'s variants) that a caller can always name as a `match` arm
/// returning a string literal — no allocation needed, and no lifetime awkwardness (this module is
/// `std`/`legacy`-feature-gated, so `String` would have been just as available if it *had* been
/// awkward).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LegacyLocationFix {
    pub latitude: f64,
    pub longitude: f64,
    pub source: &'static str,
    /// Wall-clock milliseconds at which the fix was acquired.
    pub ts_ms: u64,
}
