use dust_dds::infrastructure::type_support::TypeSupport;

use super::microgrid_controller_feature::MicrogridControllerFeature;

/// dust_dds patch of `tms::MicrogridControllerInfo` — `@nested @extensibility(APPENDABLE)` (idl:4345),
/// member-only (`ControlServiceInfo.mc`, itself `@optional`). `features` is a
/// `sequence<MicrogridControllerFeature>` — a non-primitive (enum) element sequence carrying the
/// rule-(12) NEXTINT delimiter (vendored dust_dds patch; `vendor/dust_dds/PATCHES.md`). Field order =
/// the IDL member order, matching `lib_tms::generated::MicrogridControllerInfo`. Derive-only: no hand
/// impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct MicrogridControllerInfo {
    pub features: Vec<MicrogridControllerFeature>,
    pub priority_ranking: u16,
}
