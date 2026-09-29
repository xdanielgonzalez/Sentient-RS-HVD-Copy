use super::prelude::*;

/// Derive `total_voltage_cv` from a pack's per-cell mV readings: sum of cell mV / 10 => centivolts,
/// truncating the last mV digit the same way the real 2-decimal `total_voltage` field always does
/// relative to per-cell mV precision. The single source of truth for this derivation — shared by
/// [`BatteryState::sample_14_cell`] and [`super::BatteryStateSpec::build`] (via
/// `impl_battery_state_spec.rs`) — so a deliberate `total=` override is the *only* way the two can
/// ever disagree.
pub(super) fn derive_total_voltage_cv(cells: &[u16]) -> u16 {
    (cells.iter().map(|&mv| mv as u32).sum::<u32>() / 10) as u16
}

impl BatteryState {
    /// A deterministic 14-cell/6-temp fixture with distinct, easy-to-assert values — handy for
    /// round-trip tests and e2e harnesses that don't care about a specific pack.
    pub fn sample_14_cell() -> Self {
        let mut cells: Vec<u16, MAX_CELLS> = Vec::new();
        for i in 0..14u16 {
            let _ = cells.push(3800 + i);
        }
        let mut temps: Vec<u16, MAX_TEMPS> = Vec::new();
        for i in 0..6u16 {
            // Raw Kelvin x10; 2731 + 250 => 25.0C, +i*2 => +0.2C steps.
            let _ = temps.push(2731 + 250 + i * 2);
        }
        let mut serial = String::new();
        let _ = serial.push_str("SP-SIM-0001");
        // total_voltage_cv is derived from the cells so the fixture is internally consistent (it
        // previously hardcoded 5452 = 54.52 V while the 14 cells above sum to 53.291 V) — see
        // `derive_total_voltage_cv` above.
        let total_voltage_cv = derive_total_voltage_cv(&cells);
        BatteryState {
            cell_voltages_mv: cells,
            temperatures_raw: temps,
            current_ca: -164,
            total_voltage_cv,
            remaining_capacity_cah: 1395,
            typical_capacity_cah: 1800,
            soc_dpct: 775,
            rated_capacity_cah: 1800,
            cycles: 2,
            soh_dpct: 1000,
            port_voltage_cv: 5453,
            faults: FaultFlags::default(),
            serial,
        }
    }

    /// A deterministic 16-cell/8-temp fixture — worst-case payload size for the MCU TX buffer
    /// pool boundary test. Same value structure as `sample_14_cell`, extended to the full 16-cell
    /// and 8-temp capacity to exercise the maximum serialized MQTT PUBLISH frame size.
    pub fn sample_16_cell() -> Self {
        let mut cells: Vec<u16, MAX_CELLS> = Vec::new();
        for i in 0..16u16 {
            let _ = cells.push(3800 + i);
        }
        let mut temps: Vec<u16, MAX_TEMPS> = Vec::new();
        for i in 0..8u16 {
            // Raw Kelvin x10; 2731 + 250 => 25.0C, +i*2 => +0.2C steps.
            let _ = temps.push(2731 + 250 + i * 2);
        }
        let mut serial = String::new();
        let _ = serial.push_str("SP-SIM-0001");
        let total_voltage_cv = derive_total_voltage_cv(&cells);
        BatteryState {
            cell_voltages_mv: cells,
            temperatures_raw: temps,
            current_ca: -164,
            total_voltage_cv,
            remaining_capacity_cah: 1395,
            typical_capacity_cah: 1800,
            soc_dpct: 775,
            rated_capacity_cah: 1800,
            cycles: 2,
            soh_dpct: 1000,
            port_voltage_cv: 5453,
            faults: FaultFlags::default(),
            serial,
        }
    }
}
