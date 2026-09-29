use dust_dds::infrastructure::type_support::TypeSupport;

use super::distribution_feature::DistributionFeature;

/// dust_dds patch of `tms::DistributionInfo` — `@nested @extensibility(APPENDABLE)` (idl:3588),
/// member-only (`PowerDeviceInfo.distribution`, itself `@optional`). `features` is a
/// `sequence<DistributionFeature>` — a non-primitive (enum) element sequence carrying the rule-(12)
/// NEXTINT delimiter (vendored dust_dds patch; `vendor/dust_dds/PATCHES.md`). Field order = the IDL
/// member order, matching `lib_tms::generated::DistributionInfo`. Derive-only: no hand impl, so this
/// is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct DistributionInfo {
    pub features: Vec<DistributionFeature>,
}
