use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::ThermalInfo` — `@nested @extensibility(APPENDABLE)` (idl:1254), member-only
/// (`PowerHardwareInfo.thermal`). `thermal_zone` is a `sequence<string>`; strings are non-primitive
/// sequence elements, so on the wire it carries the rule-(12) NEXTINT delimiter (vendored dust_dds
/// patch; `vendor/dust_dds/PATCHES.md`). `Vec<String>` (not `heapless`) — dust_dds's `XTypesBinding`
/// blanket covers `alloc::vec::Vec`/`alloc::String`. Field order = the IDL member order, matching
/// `lib_tms::generated::ThermalInfo`. Derive-only: no hand impl, so this is a single-type file by
/// design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct ThermalInfo {
    pub thermal_zone: Vec<String>,
}
