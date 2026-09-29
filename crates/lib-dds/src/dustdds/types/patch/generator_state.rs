use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::GeneratorState` — `@nested @extensibility(APPENDABLE)` (idl:1705),
/// member-only (`PowerHardwareUpdate.generator`, itself `@optional`). Both members are required (no
/// `@optional` here), so this sub-shape round-trips byte-identically. Field order = the IDL member
/// order, matching `lib_tms::generated::GeneratorState`. Derive-only: no hand impl, so this is a
/// single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct GeneratorState {
    pub field_current: f32,
    pub stator_temperature: f32,
}
