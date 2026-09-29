/// Time-modulated telemetry drift parameters for [`super::SeplosSim`]'s opt-in "living dashboard"
/// mode (see `impl_drift_spec.rs`): [`super::super::parse_drift_spec`] parses the runtime spec grammar,
/// and [`DriftSpec::apply`] renders a [`super::BatteryState`] at a given elapsed time. Every field
/// is a plain oscillation amplitude/period/seed — the struct carries no mutable state of its own,
/// so `apply` stays a pure function of `(self, base, elapsed)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriftSpec {
    /// Full sine-wave period for the current/cell-voltage oscillation, in seconds.
    pub period_s: u32,
    /// Peak current swing, in 10 mA (centiamps), either side of the base current.
    pub current_amp_ca: u16,
    /// Peak per-cell voltage swing, in mV, either side of each cell's base voltage.
    pub cell_amp_mv: u16,
    /// Peak temperature swing, in raw 0.1 K units, either side of each sensor's base reading.
    pub temp_amp_raw: u16,
    /// Peak pseudo-random noise added to cell mV (and, more mildly, temperature) readings, in
    /// their native units — uniform in `[-noise_mv, +noise_mv]`.
    pub noise_mv: u16,
    /// Seed for the deterministic per-cell/per-tick noise generator (`impl_drift_spec.rs`'s
    /// `noise_word`) — not a cryptographic RNG, just enough variance that a live dashboard
    /// doesn't show perfectly clean sine waves.
    pub seed: u32,
}
