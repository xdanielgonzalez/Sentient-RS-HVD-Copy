use dust_dds::infrastructure::type_support::TypeSupport;

use super::power_port_conversion_info::PowerPortConversionInfo;

/// dust_dds patch of `tms::PassiveConversionInfo` — `@nested @extensibility(APPENDABLE)` (idl:4237),
/// member-only (`ConversionInfo.passive_conversion`, itself `@optional`). `port_conversion` is a
/// `sequence<PowerPortConversionInfo>` — a non-primitive-element sequence carrying the rule-(12)
/// NEXTINT delimiter (vendored dust_dds patch; `vendor/dust_dds/PATCHES.md`). Field order = the IDL
/// member order, matching `lib_tms::generated::PassiveConversionInfo`. Derive-only: no hand impl, so
/// this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct PassiveConversionInfo {
    pub port_conversion: Vec<PowerPortConversionInfo>,
}
