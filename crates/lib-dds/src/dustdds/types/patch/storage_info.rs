use dust_dds::infrastructure::type_support::TypeSupport;

use super::energy_start_stop_level::EnergyStartStopLevel;
use super::load_sharing_info::LoadSharingInfo;
use super::storage_feature::StorageFeature;

/// dust_dds patch of `tms::StorageInfo` — `@nested @extensibility(APPENDABLE)` (idl:2151), member-only
/// (`PowerDeviceInfo.storage`, itself `@optional`). `features` (`sequence<StorageFeature>`) and
/// `supported_energy_start_stop_levels` (`sequence<EnergyStartStopLevel>`) are non-primitive (enum)
/// element sequences carrying the rule-(12) NEXTINT delimiter; `load_sharing` is a required nested
/// struct. Field order = the IDL member order, matching `lib_tms::generated::StorageInfo`. Derive-
/// only: no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct StorageInfo {
    pub features: Vec<StorageFeature>,
    pub max_charge_energy: f32,
    pub load_sharing: LoadSharingInfo,
    pub supported_energy_start_stop_levels: Vec<EnergyStartStopLevel>,
}
