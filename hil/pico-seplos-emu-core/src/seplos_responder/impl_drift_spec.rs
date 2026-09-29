use core::f64::consts::PI;

use super::impl_battery_state::derive_total_voltage_cv;
use super::prelude::*;
use super::{MAX_CELLS, MAX_TEMPS};

/// A small per-cell frequency stagger (not a fixed phase constant) so cells visibly de-correlate
/// from each other as time passes, while every cell's delta is still EXACTLY zero at `elapsed_ms == 0`
/// (`phase == 0`, and `0 * anything == 0`) — the property [`DriftSpec::apply`]'s zero-elapsed
/// determinism relies on. Each additional cell index runs a little faster than the last, capped so
/// even `MAX_CELLS` cells stay a gentle stagger.
const CELL_FREQUENCY_STAGGER: f64 = 0.04;

/// `soc_dpct`'s oscillation amplitude, in 0.1 % units (so 15 == 1.5 percentage points).
const SOC_AMPLITUDE_DPCT: f64 = 15.0;

/// Noise-generator channel tags, so cell noise and temperature noise never produce identical
/// patterns despite sharing one generator + seed.
const CHANNEL_CELL: u32 = 0;
const CHANNEL_TEMP: u32 = 1;

impl Default for DriftSpec {
    /// `period=180 current=400 cells=6 temp=20 noise=3` with a fixed seed — the same values
    /// `drift on` / `--drift on` / the chart's default `sim.drift` string produce.
    fn default() -> Self {
        DriftSpec {
            period_s: 180,
            current_amp_ca: 400,
            cell_amp_mv: 6,
            temp_amp_raw: 20,
            noise_mv: 3,
            seed: 0x5EED_5EED,
        }
    }
}

impl DriftSpec {
    /// Render `base` at `elapsed_ms` into a "live" [`BatteryState`]. Pure and deterministic: the
    /// same `(self, base, elapsed_ms)` always produces the same result — no internal RNG state, no
    /// wall-clock reads. Every arithmetic step is saturating/clamped, so this never panics.
    ///
    /// * `current_ca` and each cell's mV oscillate around `base`'s values at `self.period_s`
    ///   (cells additionally get `CELL_FREQUENCY_STAGGER` so they visibly de-correlate over
    ///   time), plus deterministic pseudo-random noise up to `self.noise_mv`.
    /// * Temperatures oscillate at half the frequency (a "slower" sine) with the same noise
    ///   amplitude — mild relative to `temp_amp_raw`'s typical scale.
    /// * `soc_dpct` oscillates slowly (a quarter the frequency, i.e. 4x the period) with a fixed
    ///   `SOC_AMPLITUDE_DPCT` amplitude, clamped to `[0, 1000]`; `remaining_capacity_cah` is
    ///   re-derived from it (`typical * soc / 1000`) rather than drifted independently.
    /// * `total_voltage_cv` is always `derive_total_voltage_cv` of the drifted cells — never
    ///   drifted on its own; `port_voltage_cv` preserves `base`'s original total-to-port offset
    ///   on top of the newly-derived total.
    /// * `cycles`, `soh_dpct`, `typical_capacity_cah`, and `rated_capacity_cah` are carried over
    ///   from `base` unchanged.
    ///
    /// At `elapsed_ms == 0` every sine term is exactly `0` (`phase == 0`), so with
    /// `self.noise_mv == 0` this returns `base` field-for-field, EXCEPT `total_voltage_cv`, which
    /// is always `derive_total_voltage_cv` of the (unchanged) cells rather than copied from
    /// `base.total_voltage_cv` directly — the two coincide whenever `base` is itself
    /// derive-consistent (true of [`BatteryState::sample_14_cell`] and every pack that didn't
    /// explicitly override `total`).
    pub fn apply(&self, base: &BatteryState, elapsed_ms: u64) -> BatteryState {
        let period_s = f64::from(self.period_s.max(1));
        let elapsed_s = elapsed_ms as f64 / 1000.0;
        let tick = (elapsed_ms / 1000).min(u64::from(u32::MAX)) as u32;
        let phase = 2.0 * PI * elapsed_s / period_s;

        let current_ca = drift_i16(
            base.current_ca,
            f64::from(self.current_amp_ca) * libm::sin(phase),
        );

        let mut cells: Vec<u16, MAX_CELLS> = Vec::new();
        for (i, &base_mv) in base.cell_voltages_mv.iter().enumerate() {
            let cell_phase = phase * (1.0 + CELL_FREQUENCY_STAGGER * i as f64);
            let wave = f64::from(self.cell_amp_mv) * libm::sin(cell_phase);
            let noise = signed_noise(self.seed, CHANNEL_CELL, i as u32, tick, self.noise_mv);
            // `base.cell_voltages_mv` is already MAX_CELLS-bounded; re-pushing one value per
            // source cell can never overflow the same-capacity Vec.
            let _ = cells.push(drift_u16(base_mv, wave + noise));
        }

        let mut temps: Vec<u16, MAX_TEMPS> = Vec::new();
        for (i, &base_raw) in base.temperatures_raw.iter().enumerate() {
            let wave = f64::from(self.temp_amp_raw) * libm::sin(phase / 2.0);
            let noise = signed_noise(self.seed, CHANNEL_TEMP, i as u32, tick, self.noise_mv);
            let _ = temps.push(drift_u16(base_raw, wave + noise));
        }

        let total_voltage_cv = derive_total_voltage_cv(&cells);
        let base_total = derive_total_voltage_cv(&base.cell_voltages_mv);
        let port_offset = i32::from(base.port_voltage_cv) - i32::from(base_total);
        let port_voltage_cv =
            (i32::from(total_voltage_cv) + port_offset).clamp(0, i32::from(u16::MAX)) as u16;

        let soc_delta = SOC_AMPLITUDE_DPCT * libm::sin(phase / 4.0);
        let soc_dpct = drift_u16(base.soc_dpct, soc_delta).clamp(0, 1000);
        let remaining_capacity_cah =
            ((u32::from(base.typical_capacity_cah) * u32::from(soc_dpct)) / 1000) as u16;

        BatteryState {
            cell_voltages_mv: cells,
            temperatures_raw: temps,
            current_ca,
            total_voltage_cv,
            remaining_capacity_cah,
            typical_capacity_cah: base.typical_capacity_cah,
            soc_dpct,
            rated_capacity_cah: base.rated_capacity_cah,
            cycles: base.cycles,
            soh_dpct: base.soh_dpct,
            port_voltage_cv,
        }
    }
}

/// Add a floating-point delta to a `u16` base and land back in `u16`, saturating (not wrapping)
/// at both ends and truncating toward zero (not rounding), so amplitude-bounded callers can prove
/// the result never overshoots their bound by rounding up past it. Rust's `as` float->int cast is
/// defined as saturating since 1.45 — an out-of-range value clamps to the type's bound and `NaN`
/// maps to `0` — so this can never panic or produce a garbage value.
fn drift_u16(base: u16, delta: f64) -> u16 {
    (f64::from(base) + delta) as u16
}

/// As [`drift_u16`], landing in `i16` instead (for the signed `current_ca` field).
fn drift_i16(base: i16, delta: f64) -> i16 {
    (f64::from(base) + delta) as i16
}

/// A tiny deterministic xorshift32 + LCG generator — NOT cryptographic, just enough variance that
/// a "living" simulated dashboard doesn't show perfectly clean sine waves (no external `rand`
/// dependency). Same `(seed, channel, index, tick)` always yields the same word.
fn noise_word(seed: u32, channel: u32, index: u32, tick: u32) -> u32 {
    let mut x = seed
        ^ channel.wrapping_mul(0x2545_F491)
        ^ index.wrapping_mul(0x9E37_79B9)
        ^ tick.wrapping_mul(0x85EB_CA6B);
    if x == 0 {
        // xorshift is fixed at 0 forever once it reaches 0; nudge so an all-zero mix (e.g.
        // seed=0, channel=0, index=0, tick=0) still varies over subsequent ticks/indices.
        x = 0x9E37_79B9;
    }
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223)
}

/// A pseudo-random value in `[-amp, amp]` (inclusive), deterministic in `(seed, channel, index,
/// tick)`. `amp == 0` always returns `0.0` without touching the generator, so a `noise_mv == 0`
/// spec is exactly noise-free (not merely "close").
fn signed_noise(seed: u32, channel: u32, index: u32, tick: u32, amp: u16) -> f64 {
    if amp == 0 {
        return 0.0;
    }
    let span = u32::from(amp) * 2 + 1;
    let r = noise_word(seed, channel, index, tick) % span;
    f64::from(r as i32 - i32::from(amp))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    // -------------------------------------------------------------------------------------
    // DriftSpec::apply
    // -------------------------------------------------------------------------------------

    #[test]
    fn apply_is_deterministic() {
        let base = BatteryState::sample_14_cell();
        let spec = DriftSpec::default();
        let elapsed_ms = 12_345;
        assert_eq!(spec.apply(&base, elapsed_ms), spec.apply(&base, elapsed_ms));
    }

    #[test]
    fn apply_at_zero_elapsed_with_no_noise_equals_base_except_the_derived_total() {
        let base = BatteryState::sample_14_cell();
        let spec = DriftSpec {
            noise_mv: 0,
            ..DriftSpec::default()
        };
        let drifted = spec.apply(&base, 0);

        assert_eq!(drifted.cell_voltages_mv, base.cell_voltages_mv);
        assert_eq!(drifted.temperatures_raw, base.temperatures_raw);
        assert_eq!(drifted.current_ca, base.current_ca);
        assert_eq!(drifted.soc_dpct, base.soc_dpct);
        assert_eq!(drifted.remaining_capacity_cah, base.remaining_capacity_cah);
        assert_eq!(drifted.port_voltage_cv, base.port_voltage_cv);
        assert_eq!(drifted.cycles, base.cycles);
        assert_eq!(drifted.soh_dpct, base.soh_dpct);
        // `sample_14_cell` is derive-consistent by construction, so the re-derived total equals
        // `base.total_voltage_cv` too — asserted explicitly via `derive_total_voltage_cv` (the
        // property `apply` actually guarantees) rather than assumed.
        assert_eq!(
            drifted.total_voltage_cv,
            derive_total_voltage_cv(&drifted.cell_voltages_mv)
        );
        assert_eq!(drifted.total_voltage_cv, base.total_voltage_cv);
    }

    #[test]
    fn apply_total_voltage_always_matches_derive_of_the_drifted_cells() {
        let base = BatteryState::sample_14_cell();
        let spec = DriftSpec::default();
        for secs in [0u64, 1, 45, 90, 180, 1_000] {
            let elapsed_ms = secs * 1000;
            let drifted = spec.apply(&base, elapsed_ms);
            assert_eq!(
                drifted.total_voltage_cv,
                derive_total_voltage_cv(&drifted.cell_voltages_mv),
                "mismatch at elapsed={secs}s"
            );
        }
    }

    #[test]
    fn apply_preserves_cycles_and_soh() {
        let mut base = BatteryState::sample_14_cell();
        base.cycles = 42;
        base.soh_dpct = 980;
        let spec = DriftSpec::default();
        let drifted = spec.apply(&base, 37_000);
        assert_eq!(drifted.cycles, base.cycles);
        assert_eq!(drifted.soh_dpct, base.soh_dpct);
        assert_eq!(drifted.rated_capacity_cah, base.rated_capacity_cah);
        assert_eq!(drifted.typical_capacity_cah, base.typical_capacity_cah);
    }

    #[test]
    fn apply_cell_values_stay_bounded_by_base_plus_amplitude_and_noise() {
        let base = BatteryState::sample_14_cell();
        let spec = DriftSpec::default();
        let bound = i32::from(spec.cell_amp_mv) + i32::from(spec.noise_mv);
        for secs in 0..spec.period_s as u64 * 2 {
            let elapsed_ms = secs * 1000;
            let drifted = spec.apply(&base, elapsed_ms);
            for (base_mv, drifted_mv) in base
                .cell_voltages_mv
                .iter()
                .zip(drifted.cell_voltages_mv.iter())
            {
                let delta = i32::from(*drifted_mv) - i32::from(*base_mv);
                assert!(
                    delta.abs() <= bound,
                    "cell delta {delta} exceeds bound {bound} at elapsed={secs}s"
                );
            }
        }
    }

    #[test]
    fn apply_soc_is_clamped_to_the_valid_dpct_range() {
        let low_base = BatteryState {
            soc_dpct: 5,
            ..BatteryState::sample_14_cell()
        };
        let high_base = BatteryState {
            soc_dpct: 998,
            ..BatteryState::sample_14_cell()
        };
        let spec = DriftSpec {
            period_s: 4,
            noise_mv: 0,
            ..DriftSpec::default()
        };

        // soc's phase is `phase / 4` where `phase == 2*pi*elapsed_s/period_s`, i.e.
        // `elapsed_s == 2*theta*period_s/pi`. `theta == 3*pi/2` (sin == -1) => elapsed_s == 12
        // for period_s == 4.
        let low_drifted = spec.apply(&low_base, 12_000);
        assert_eq!(low_drifted.soc_dpct, 0, "must clamp at the floor, not wrap");

        // `theta == pi/2` (sin == +1) => elapsed_s == 4 for period_s == 4.
        let high_drifted = spec.apply(&high_base, 4_000);
        assert_eq!(
            high_drifted.soc_dpct, 1000,
            "must clamp at the ceiling, not overflow"
        );
    }

    #[test]
    fn apply_remaining_capacity_tracks_soc_and_typical_capacity() {
        let base = BatteryState::sample_14_cell();
        let spec = DriftSpec::default();
        let drifted = spec.apply(&base, 90_000);
        let expected =
            (u32::from(drifted.typical_capacity_cah) * u32::from(drifted.soc_dpct)) / 1000;
        assert_eq!(u32::from(drifted.remaining_capacity_cah), expected);
    }
}
