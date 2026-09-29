use heapless::Vec;

use super::super::{MAX_CELLS, MAX_TEMPS};

/// Canned battery telemetry the responder encodes into `0x42` replies. Constructor
/// (`sample_14_cell`) lives in `impl_battery_state.rs` (type/impl split convention).
#[derive(Debug, Clone, PartialEq)]
pub struct BatteryState {
    pub cell_voltages_mv: Vec<u16, MAX_CELLS>,
    pub temperatures_raw: Vec<u16, MAX_TEMPS>,
    pub current_ca: i16,
    pub total_voltage_cv: u16,
    pub remaining_capacity_cah: u16,
    pub typical_capacity_cah: u16,
    pub soc_dpct: u16,
    pub rated_capacity_cah: u16,
    pub cycles: u16,
    pub soh_dpct: u16,
    pub port_voltage_cv: u16,
}
