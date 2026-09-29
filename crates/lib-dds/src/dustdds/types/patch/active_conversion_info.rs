use dust_dds::infrastructure::type_support::TypeSupport;

use super::load_sharing_info::LoadSharingInfo;

/// dust_dds patch of `tms::ActiveConversionInfo` — `@nested @extensibility(APPENDABLE)` (idl:4152),
/// member-only (`ConversionInfo.active_conversion`, itself `@optional`). `load_sharing` is a
/// `sequence<LoadSharingInfo>` — a non-primitive-element sequence carrying the rule-(12) NEXTINT
/// delimiter (vendored dust_dds patch; `vendor/dust_dds/PATCHES.md`). Field order = the IDL member
/// order, matching `lib_tms::generated::ActiveConversionInfo`. Derive-only: no hand impl, so this is
/// a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct ActiveConversionInfo {
    pub load_sharing: Vec<LoadSharingInfo>,
}
