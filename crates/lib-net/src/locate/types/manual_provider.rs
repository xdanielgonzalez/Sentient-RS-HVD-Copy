/// The manual-override tier: a fixed, config-supplied `(lat, lon)` — highest priority by baseline
/// convention (see the crate's module doc comment). No I/O; `locate()` always succeeds and simply
/// stamps a fresh timestamp (see `impl_manual.rs`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ManualProvider {
    pub lat: f64,
    pub lon: f64,
}
