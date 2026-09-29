use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::FuelInfo` — `@nested @extensibility(APPENDABLE)` (idl:1903), member-only
/// (`PowerHardwareInfo.fuel`). Both members required. Field order = the IDL member order, matching
/// `lib_tms::generated::FuelInfo`. Derive-only: no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct FuelInfo {
    pub max_fuel_level: f32,
    pub low_fuel_level_cutoff: f32,
}
