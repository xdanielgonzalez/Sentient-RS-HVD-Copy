use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::StorageCellState` — `@nested @extensibility(APPENDABLE)` (idl:1496),
/// member-only (`EnergyStorageState.storage_cells`, a `sequence<StorageCellState>`). `state_of_charge`
/// is `@optional float` → `Option<f32>` + `#[dust_dds(optional)]` (wp-derive; the XCDR2 serializer
/// emits a 1-byte presence flag before it, wp-serializer). The other members are required. Field
/// order = the IDL member order, matching `lib_tms::generated::StorageCellState`. Derive-only: no
/// hand impl, so this is a single-type file by design.
///
/// Exclusive to the `PowerHardwareUpdate` closure (via `EnergyStorageState`); authored here by
/// wp-topic-powerhardware. Not in the original WP file list — flagged for wp-patch-mod wiring.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct StorageCellState {
    pub cell_number: u16,
    pub cycle_counter: f32,
    #[dust_dds(optional)]
    pub state_of_charge: Option<f32>,
    pub temperature: f32,
    pub voltage: f32,
}
