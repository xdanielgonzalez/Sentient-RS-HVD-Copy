use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::PowerElectronicsState` — `@nested @extensibility(APPENDABLE)` (idl:1464),
/// member-only (`PowerHardwareUpdate.power_electronics`, itself `@optional`). Single required member
/// (no `@optional` here), so this sub-shape round-trips byte-identically. Matches
/// `lib_tms::generated::PowerElectronicsState`. Derive-only: no hand impl, so this is a single-type
/// file by design.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct PowerElectronicsState {
    pub temperature: f32,
}
