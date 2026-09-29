use dust_dds::infrastructure::type_support::TypeSupport;

use super::grounding_design_type::GroundingDesignType;
use super::power_switch_feature::PowerSwitchFeature;

/// dust_dds patch of `tms::GroundingInfo` — `@nested @extensibility(APPENDABLE)` (idl:2935),
/// member-only (`PowerDeviceInfo.grounds`). `protected_ports` is a `sequence<long>` (primitive
/// element — plain length-prefixed, no NEXTINT); `control_switch_features`/`pulse_switch_features` are
/// `sequence<PowerSwitchFeature>` (non-primitive enum elements — carry the rule-(12) NEXTINT
/// delimiter; vendored dust_dds patch, `vendor/dust_dds/PATCHES.md`). Field order = the IDL member
/// order, matching `lib_tms::generated::GroundingInfo`. Derive-only: no hand impl, so this is a
/// single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct GroundingInfo {
    pub ground_number: i32,
    pub ground_type: GroundingDesignType,
    pub protected_ports: Vec<i32>,
    pub control_switch_features: Vec<PowerSwitchFeature>,
    pub pulse_switch_features: Vec<PowerSwitchFeature>,
}
