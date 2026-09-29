use dust_dds::infrastructure::type_support::TypeSupport;

use super::active_conversion_info::ActiveConversionInfo;
use super::conversion_feature::ConversionFeature;
use super::passive_conversion_info::PassiveConversionInfo;

/// dust_dds patch of `tms::ConversionInfo` — `@nested @extensibility(APPENDABLE)` (idl:4252),
/// member-only (`PowerDeviceInfo.conversion`, itself `@optional`). `features` is a
/// `sequence<ConversionFeature>` (non-primitive enum elements — rule-(12) NEXTINT delimiter);
/// `active_conversion`/`passive_conversion` are `@optional` nested structs → `Option<T>` +
/// `#[dust_dds(optional)]` (wp-derive; XCDR2 presence flag from wp-serializer). Field order = the IDL
/// member order, matching `lib_tms::generated::ConversionInfo`. Derive-only: no hand impl, so this is
/// a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct ConversionInfo {
    pub features: Vec<ConversionFeature>,
    #[dust_dds(optional)]
    pub active_conversion: Option<ActiveConversionInfo>,
    #[dust_dds(optional)]
    pub passive_conversion: Option<PassiveConversionInfo>,
}
