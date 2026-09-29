use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::ThermalState` — `@nested @extensibility(APPENDABLE)` (idl:1269),
/// member-only (`PowerHardwareUpdate.thermal`, itself `@optional`). Single required member
/// `thermal_load`, a `sequence<float>` → `Vec<f32>` (no `@optional` here). Matches
/// `lib_tms::generated::ThermalState`. Derive-only: no hand impl, so this is a single-type file by
/// design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct ThermalState {
    pub thermal_load: Vec<f32>,
}
