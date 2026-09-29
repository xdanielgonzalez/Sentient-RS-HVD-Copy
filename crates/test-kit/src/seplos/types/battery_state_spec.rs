use heapless::Vec;

use super::super::{MAX_CELLS, MAX_TEMPS};

/// A runtime-definable overlay atop [`super::BatteryState`]'s neutral baseline
/// ([`super::BatteryState::sample_14_cell`]).
///
/// Every field starts `None` ("inherit the baseline value") and is overridden individually,
/// either through the fluent builder methods in `impl_battery_state_spec.rs` (for Rust callers)
/// or by parsing the textual `key=value` grammar accepted by
/// [`super::super::parse_battery_state_spec`] (for the stdin control protocol, a coming cucumber
/// harness, and Robot Framework keywords — one shared parser, one shape of truth). See
/// [`super::super::parse_battery_state_spec`]'s doc comment for the full grammar and unit definitions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BatteryStateSpec {
    pub(crate) cells_mv: Option<Vec<u16, MAX_CELLS>>,
    pub(crate) temps_raw: Option<Vec<u16, MAX_TEMPS>>,
    pub(crate) current_ca: Option<i16>,
    pub(crate) total_voltage_cv: Option<u16>,
    pub(crate) remaining_capacity_cah: Option<u16>,
    pub(crate) typical_capacity_cah: Option<u16>,
    pub(crate) soc_dpct: Option<u16>,
    pub(crate) rated_capacity_cah: Option<u16>,
    pub(crate) cycles: Option<u16>,
    pub(crate) soh_dpct: Option<u16>,
    pub(crate) port_voltage_cv: Option<u16>,
}
