use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::EnergyStorageInfo` — `@nested @extensibility(APPENDABLE)` (idl:1607),
/// member-only (`PowerHardwareInfo.energy_storage`). `high_state_of_charge`/`low_state_of_charge` are
/// `@optional float` → `Option<f32>` + `#[dust_dds(optional)]` (enabled by the wp-derive
/// `@optional` patch; the XCDR2 serializer emits a 1-byte presence flag before each, wp-serializer).
/// Field order = the IDL member order, matching `lib_tms::generated::EnergyStorageInfo`. Derive-only:
/// no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct EnergyStorageInfo {
    #[dust_dds(optional)]
    pub high_state_of_charge: Option<f32>,
    #[dust_dds(optional)]
    pub low_state_of_charge: Option<f32>,
    pub min_temperature: f32,
    pub nom_temperature: f32,
    pub max_temperature: f32,
}
