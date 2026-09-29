use dust_dds::infrastructure::type_support::TypeSupport;

use super::conversion_info::ConversionInfo;
use super::distribution_info::DistributionInfo;
use super::grounding_info::GroundingInfo;
use super::load_info::LoadInfo;
use super::power_port_info::PowerPortInfo;
use super::source_info::SourceInfo;
use super::storage_info::StorageInfo;

/// dust_dds patch of `tms::PowerDeviceInfo` — `@nested @extensibility(APPENDABLE)` (idl:5234),
/// member-only (`DeviceInfo.power_device`, itself `@optional`). `power_ports`
/// (`sequence<PowerPortInfo>`) and `grounds` (`sequence<GroundingInfo>`) are required non-primitive-
/// element sequences carrying the rule-(12) NEXTINT delimiter; `conversion`/`distribution`/`source`/
/// `storage`/`load` are `@optional` nested structs → `Option<T>` + `#[dust_dds(optional)]` (wp-derive;
/// XCDR2 presence flag from wp-serializer). Field order = the IDL member order, matching
/// `lib_tms::generated::PowerDeviceInfo`. Derive-only: no hand impl, so this is a single-type file by
/// design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct PowerDeviceInfo {
    pub power_ports: Vec<PowerPortInfo>,
    pub grounds: Vec<GroundingInfo>,
    #[dust_dds(optional)]
    pub conversion: Option<ConversionInfo>,
    #[dust_dds(optional)]
    pub distribution: Option<DistributionInfo>,
    #[dust_dds(optional)]
    pub source: Option<SourceInfo>,
    #[dust_dds(optional)]
    pub storage: Option<StorageInfo>,
    #[dust_dds(optional)]
    pub load: Option<LoadInfo>,
}
