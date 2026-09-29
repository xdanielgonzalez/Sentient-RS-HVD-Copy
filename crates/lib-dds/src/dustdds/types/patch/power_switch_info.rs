use dust_dds::infrastructure::type_support::TypeSupport;

use super::power_switch_feature::PowerSwitchFeature;

/// dust_dds patch of `tms::PowerSwitchInfo` — `@nested @extensibility(APPENDABLE)` (idl:2665),
/// member-only (`PowerPortInfo.switch_info`, itself `@optional`). `features` is a
/// `sequence<PowerSwitchFeature>` — a non-primitive (enum) element sequence carrying the rule-(12)
/// NEXTINT delimiter (vendored dust_dds patch; `vendor/dust_dds/PATCHES.md`). Field order = the IDL
/// member order, matching `lib_tms::generated::PowerSwitchInfo`. Derive-only: no hand impl, so this
/// is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct PowerSwitchInfo {
    pub features: Vec<PowerSwitchFeature>,
    pub interrupt_amperage: f32,
}
