use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::GeneratorInfo` — `@nested @extensibility(APPENDABLE)` (idl:1671),
/// member-only (`PowerHardwareInfo.generator`). All members required. Field order = the IDL member
/// order, matching `lib_tms::generated::GeneratorInfo`. Derive-only: no hand impl, so this is a
/// single-type file by design.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct GeneratorInfo {
    pub min_field_current: f32,
    pub max_field_current: f32,
    pub max_stator_temperature: f32,
}
