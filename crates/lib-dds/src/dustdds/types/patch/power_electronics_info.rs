use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::PowerElectronicsInfo` — `@nested @extensibility(APPENDABLE)` (idl:1434),
/// member-only (`PowerHardwareInfo.power_electronics`). Both members required. Field order = the IDL
/// member order, matching `lib_tms::generated::PowerElectronicsInfo`. Derive-only: no hand impl, so
/// this is a single-type file by design.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct PowerElectronicsInfo {
    pub min_temperature: f32,
    pub max_temperature: f32,
}
