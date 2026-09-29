use super::impl_battery_state::derive_total_voltage_cv;
use super::prelude::{BatteryState, BatteryStateSpec, MAX_CELLS, MAX_TEMPS};
use heapless::Vec;

impl BatteryStateSpec {
    /// A fresh spec with every field unset (inherits [`BatteryState::sample_14_cell`] entirely).
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the per-cell voltages (mV). Fails closed: an empty list, or more than
    /// [`MAX_CELLS`] entries, is rejected rather than silently truncated/defaulted.
    pub fn cells(mut self, cells_mv: &[u16]) -> Result<Self, String> {
        if cells_mv.is_empty() {
            return Err("cells must not be empty".to_string());
        }
        if cells_mv.len() > MAX_CELLS {
            return Err(format!(
                "too many cells: {} > MAX_CELLS ({MAX_CELLS})",
                cells_mv.len()
            ));
        }
        let mut v: Vec<u16, MAX_CELLS> = Vec::new();
        for &mv in cells_mv {
            // Unreachable given the length check above, but never panic on a push failure.
            v.push(mv)
                .map_err(|_| "cells: internal capacity overflow".to_string())?;
        }
        self.cells_mv = Some(v);
        Ok(self)
    }

    /// Override the raw temperature readings (0.1 K). Fails closed: an empty list, or more than
    /// [`MAX_TEMPS`] entries, is rejected.
    pub fn temps(mut self, temps_raw: &[u16]) -> Result<Self, String> {
        if temps_raw.is_empty() {
            return Err("temps must not be empty".to_string());
        }
        if temps_raw.len() > MAX_TEMPS {
            return Err(format!(
                "too many temps: {} > MAX_TEMPS ({MAX_TEMPS})",
                temps_raw.len()
            ));
        }
        let mut v: Vec<u16, MAX_TEMPS> = Vec::new();
        for &t in temps_raw {
            v.push(t)
                .map_err(|_| "temps: internal capacity overflow".to_string())?;
        }
        self.temps_raw = Some(v);
        Ok(self)
    }

    /// Override the pack current (10 mA, signed; negative = discharge).
    pub fn current_ca(mut self, v: i16) -> Self {
        self.current_ca = Some(v);
        self
    }

    /// Explicitly override `total_voltage_cv` (10 mV / centivolts) instead of deriving it from
    /// `cells` — the only way to build a deliberately-inconsistent (negative-test) pack.
    pub fn total_voltage_cv(mut self, v: u16) -> Self {
        self.total_voltage_cv = Some(v);
        self
    }

    /// Override the remaining capacity (10 mAh).
    pub fn remaining_capacity_cah(mut self, v: u16) -> Self {
        self.remaining_capacity_cah = Some(v);
        self
    }

    /// Override the typical capacity (10 mAh).
    pub fn typical_capacity_cah(mut self, v: u16) -> Self {
        self.typical_capacity_cah = Some(v);
        self
    }

    /// Override the state of charge (0.1 %).
    pub fn soc_dpct(mut self, v: u16) -> Self {
        self.soc_dpct = Some(v);
        self
    }

    /// Override the rated capacity (10 mAh).
    pub fn rated_capacity_cah(mut self, v: u16) -> Self {
        self.rated_capacity_cah = Some(v);
        self
    }

    /// Override the charge-cycle count.
    pub fn cycles(mut self, v: u16) -> Self {
        self.cycles = Some(v);
        self
    }

    /// Override the state of health (0.1 %).
    pub fn soh_dpct(mut self, v: u16) -> Self {
        self.soh_dpct = Some(v);
        self
    }

    /// Override the port voltage (10 mV / centivolts).
    pub fn port_voltage_cv(mut self, v: u16) -> Self {
        self.port_voltage_cv = Some(v);
        self
    }

    /// Materialize into a [`BatteryState`], filling every unset field from the neutral baseline
    /// [`BatteryState::sample_14_cell`] (a deliberate choice — not, say, all-zero — so a spec that
    /// overrides only one field still yields a fully plausible, internally-consistent pack).
    /// `total_voltage_cv` defaults to `derive_total_voltage_cv` of the (possibly-overridden)
    /// `cells`, unless explicitly overridden via [`Self::total_voltage_cv`]. `faults` and `serial`
    /// are not part of this spec (faults are set separately via the `faults` control verb) and
    /// always come from the baseline. Never panics — the only failure paths are in
    /// [`Self::cells`]/[`Self::temps`], already surfaced at call time.
    pub fn build(self) -> Result<BatteryState, String> {
        let baseline = BatteryState::sample_14_cell();
        let cells = self.cells_mv.unwrap_or(baseline.cell_voltages_mv);
        let temps = self.temps_raw.unwrap_or(baseline.temperatures_raw);
        let total_voltage_cv = self
            .total_voltage_cv
            .unwrap_or_else(|| derive_total_voltage_cv(&cells));
        Ok(BatteryState {
            cell_voltages_mv: cells,
            temperatures_raw: temps,
            current_ca: self.current_ca.unwrap_or(baseline.current_ca),
            total_voltage_cv,
            remaining_capacity_cah: self
                .remaining_capacity_cah
                .unwrap_or(baseline.remaining_capacity_cah),
            typical_capacity_cah: self
                .typical_capacity_cah
                .unwrap_or(baseline.typical_capacity_cah),
            soc_dpct: self.soc_dpct.unwrap_or(baseline.soc_dpct),
            rated_capacity_cah: self
                .rated_capacity_cah
                .unwrap_or(baseline.rated_capacity_cah),
            cycles: self.cycles.unwrap_or(baseline.cycles),
            soh_dpct: self.soh_dpct.unwrap_or(baseline.soh_dpct),
            port_voltage_cv: self.port_voltage_cv.unwrap_or(baseline.port_voltage_cv),
            faults: baseline.faults,
            serial: baseline.serial,
        })
    }
}

/// Parse the textual `key=value` battery-state spec grammar shared by the `state <spec>` /
/// `state set <spec>` stdin control verb (`seplos-tcp-sim`), a coming cucumber harness, and Robot
/// Framework keywords — one parser, so all three drive [`crate::SeplosSim`] identically.
///
/// # Grammar
///
/// Whitespace-separated `key=value` pairs, single-line (stdin-protocol-friendly), e.g.:
///
/// ```text
/// cells=3800,3805,3810 temps=2981,2983 current=-164 soc=775 soh=1000 cycles=2 remaining=1750 \
///     typical=1800 rated=1800 port=5453
/// ```
///
/// | key         | unit                              | bounds                    |
/// |-------------|-----------------------------------|---------------------------|
/// | `cells`     | comma-separated per-cell mV        | `1..=MAX_CELLS` entries   |
/// | `temps`     | comma-separated raw 0.1 K          | `1..=MAX_TEMPS` entries   |
/// | `current`   | 10 mA, signed (negative=discharge) | fits `i16`                |
/// | `soc`/`soh` | 0.1 %                              | fits `u16`                |
/// | `cycles`    | charge-cycle count                 | fits `u16`                |
/// | `remaining`/`typical`/`rated` | 10 mAh                | fits `u16`                |
/// | `port`      | 10 mV / centivolts                 | fits `u16`                |
/// | `total`     | 10 mV / centivolts (OVERRIDE)       | fits `u16`                |
///
/// Every key is optional; an unspecified key defaults from the neutral baseline
/// [`BatteryState::sample_14_cell`] (a documented, deliberate choice — see [`BatteryStateSpec::build`]).
/// `total` deliberately overrides the default `total_voltage_cv` derivation (sum of `cells` / 10)
/// — use it to build a deliberately-inconsistent (negative-test) pack. Faults and the serial
/// number are not part of this grammar; set faults separately via the `faults` control verb.
///
/// # Fail-closed
///
/// Returns `Err(String)` — never panics — for: an unrecognized key, a non-`key=value` token, a
/// non-numeric value, or a `cells`/`temps` count of `0` or `> MAX_CELLS`/`MAX_TEMPS`.
pub fn parse_battery_state_spec(spec: &str) -> Result<BatteryState, String> {
    let mut builder = BatteryStateSpec::new();
    for token in spec.split_whitespace() {
        let (key, value) = token.split_once('=').ok_or_else(|| {
            format!("invalid battery-state spec token (want key=value): {token:?}")
        })?;
        builder = apply_spec_key(builder, key, value)?;
    }
    builder.build()
}

fn apply_spec_key(
    builder: BatteryStateSpec,
    key: &str,
    value: &str,
) -> Result<BatteryStateSpec, String> {
    match key {
        "cells" => {
            let cells = parse_num_list(key, value)?;
            builder.cells(&cells)
        }
        "temps" => {
            let temps = parse_num_list(key, value)?;
            builder.temps(&temps)
        }
        "current" => Ok(builder.current_ca(parse_num(key, value)?)),
        "total" => Ok(builder.total_voltage_cv(parse_num(key, value)?)),
        "remaining" => Ok(builder.remaining_capacity_cah(parse_num(key, value)?)),
        "typical" => Ok(builder.typical_capacity_cah(parse_num(key, value)?)),
        "soc" => Ok(builder.soc_dpct(parse_num(key, value)?)),
        "rated" => Ok(builder.rated_capacity_cah(parse_num(key, value)?)),
        "cycles" => Ok(builder.cycles(parse_num(key, value)?)),
        "soh" => Ok(builder.soh_dpct(parse_num(key, value)?)),
        "port" => Ok(builder.port_voltage_cv(parse_num(key, value)?)),
        other => Err(format!("unknown battery-state spec key: {other:?}")),
    }
}

fn parse_num<T: core::str::FromStr>(key: &str, value: &str) -> Result<T, String> {
    value
        .parse::<T>()
        .map_err(|_| format!("battery-state spec key {key:?}: invalid number {value:?}"))
}

/// Parse a comma-separated list of `u16`s (host-side text scratch only — never on the wire-encode
/// path, so a plain `std::vec::Vec` is fine here despite the module's no_std-in-spirit posture
/// elsewhere).
fn parse_num_list(key: &str, value: &str) -> Result<std::vec::Vec<u16>, String> {
    if value.is_empty() {
        return Err(format!(
            "battery-state spec key {key:?}: value must not be empty"
        ));
    }
    value
        .split(',')
        .map(|s| {
            s.parse::<u16>()
                .map_err(|_| format!("battery-state spec key {key:?}: invalid value {s:?}"))
        })
        .collect()
}
