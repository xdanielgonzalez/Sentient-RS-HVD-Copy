use super::prelude::*;

/// Derive `total_voltage_cv` from a pack's per-cell mV readings: sum of cell mV / 10 => centivolts,
/// truncating the last mV digit the same way the real 2-decimal `total_voltage` field always does
/// relative to per-cell mV precision. The single source of truth for this derivation — shared by
/// [`BatteryState::sample_14_cell`] and [`super::DriftSpec::apply`] — so a deliberate `total=`
/// override is the *only* way the two can ever disagree.
pub(super) fn derive_total_voltage_cv(cells: &[u16]) -> u16 {
    (cells.iter().map(|&mv| mv as u32).sum::<u32>() / 10) as u16
}

impl BatteryState {
    /// The one canned fixture phase 1 needs: 14 cells (3800..=3813 mV), 6 temps, discharging at
    /// -164 cA, 77.5% SOC — exact values ported from `test-kit`'s `BatteryState::sample_14_cell`.
    pub fn sample_14_cell() -> Self {
        let mut cell_voltages_mv = Vec::new();
        for i in 0..14u16 {
            let _ = cell_voltages_mv.push(3800 + i);
        }

        let mut temperatures_raw = Vec::new();
        for i in 0..6u16 {
            let _ = temperatures_raw.push(2731 + 250 + i * 2);
        }

        // total_voltage_cv is derived from summing cell_voltages_mv and dividing by 10,
        // not hardcoded, to keep it in sync with the cell voltages.
        let total_voltage_cv = derive_total_voltage_cv(&cell_voltages_mv);

        Self {
            cell_voltages_mv,
            temperatures_raw,
            current_ca: -164,
            total_voltage_cv,
            remaining_capacity_cah: 1395,
            typical_capacity_cah: 1800,
            soc_dpct: 775,
            rated_capacity_cah: 1800,
            cycles: 2,
            soh_dpct: 1000,
            port_voltage_cv: 5453,
        }
    }
}
