use dust_dds::infrastructure::type_support::TypeSupport;

use super::storage_cell_state::StorageCellState;

/// dust_dds patch of `tms::EnergyStorageState` — `@nested @extensibility(APPENDABLE)` (idl:1555),
/// member-only (`PowerHardwareUpdate.energy_storage`, itself `@optional`). All scalar members are
/// required; `storage_cells` is a required `sequence<StorageCellState>` → `Vec<StorageCellState>`
/// (the per-cell `@optional state_of_charge` lives inside `StorageCellState`). Field order = the IDL
/// member order, matching `lib_tms::generated::EnergyStorageState`. Derive-only: no hand impl, so
/// this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct EnergyStorageState {
    pub min_temperature: f32,
    pub average_temperature: f32,
    pub max_temperature: f32,
    pub run_time: f32,
    pub storage_cells: Vec<StorageCellState>,
}
