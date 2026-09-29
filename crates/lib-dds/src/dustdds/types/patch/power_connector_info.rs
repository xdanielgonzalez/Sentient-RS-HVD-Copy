use dust_dds::infrastructure::type_support::TypeSupport;

use super::power_connector_feature::PowerConnectorFeature;
use super::power_connector_polarity::PowerConnectorPolarity;
use super::power_connector_type::PowerConnectorType;

/// dust_dds patch of `tms::PowerConnectorInfo` — `@nested @extensibility(APPENDABLE)` (idl:2640),
/// member-only (`PowerPortInfo.connector_info`). `features` is a `sequence<PowerConnectorFeature>` — a
/// non-primitive (enum) element sequence carrying the rule-(12) NEXTINT delimiter (vendored dust_dds
/// patch; `vendor/dust_dds/PATCHES.md`). Field order = the IDL member order, matching
/// `lib_tms::generated::PowerConnectorInfo`. Derive-only: no hand impl, so this is a single-type file
/// by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct PowerConnectorInfo {
    pub features: Vec<PowerConnectorFeature>,
    pub physical_type: PowerConnectorType,
    pub polarity: PowerConnectorPolarity,
}
