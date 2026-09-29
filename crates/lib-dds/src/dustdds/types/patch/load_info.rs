use dust_dds::infrastructure::type_support::TypeSupport;

use super::load_feature::LoadFeature;

/// dust_dds patch of `tms::LoadInfo` — `@nested @extensibility(APPENDABLE)` (idl:3693), member-only
/// (`PowerDeviceInfo.load`, itself `@optional`). `features` is a `sequence<LoadFeature>` — a
/// non-primitive (enum) element sequence carrying the rule-(12) NEXTINT delimiter (vendored dust_dds
/// patch; `vendor/dust_dds/PATCHES.md`). Field order = the IDL member order, matching
/// `lib_tms::generated::LoadInfo`. Derive-only: no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct LoadInfo {
    pub features: Vec<LoadFeature>,
    pub max_real_power: f32,
    pub max_reactive_power: f32,
}
