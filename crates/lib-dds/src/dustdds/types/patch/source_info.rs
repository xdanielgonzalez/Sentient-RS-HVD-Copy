use dust_dds::infrastructure::type_support::TypeSupport;

use super::energy_start_stop_level::EnergyStartStopLevel;
use super::load_sharing_info::LoadSharingInfo;
use super::source_feature::SourceFeature;

/// dust_dds patch of `tms::SourceInfo` — `@nested @extensibility(APPENDABLE)` (idl:3188), member-only
/// (`PowerDeviceInfo.source`, itself `@optional`). `features` (`sequence<SourceFeature>`) and
/// `supported_energy_start_stop_levels` (`sequence<EnergyStartStopLevel>`) are non-primitive (enum)
/// element sequences carrying the rule-(12) NEXTINT delimiter; `load_sharing` is a required nested
/// struct. Field order = the IDL member order, matching `lib_tms::generated::SourceInfo`. Derive-only:
/// no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct SourceInfo {
    pub features: Vec<SourceFeature>,
    pub load_sharing: LoadSharingInfo,
    pub supported_energy_start_stop_levels: Vec<EnergyStartStopLevel>,
}
