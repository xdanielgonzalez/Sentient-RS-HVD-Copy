use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::FuelState` — `@nested @extensibility(APPENDABLE)` (idl:1931),
/// member-only (`PowerHardwareUpdate.fuel`, itself `@optional`). Both members are required (no
/// `@optional` here), so this sub-shape round-trips byte-identically. Field order = the IDL member
/// order, matching `lib_tms::generated::FuelState`. Derive-only: no hand impl, so this is a
/// single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct FuelState {
    pub fuel_level: f32,
    pub fuel_pump_running: bool,
}
