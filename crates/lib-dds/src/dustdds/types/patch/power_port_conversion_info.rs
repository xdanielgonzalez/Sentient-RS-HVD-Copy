use dust_dds::infrastructure::type_support::TypeSupport;

use super::conversion_tap_info::ConversionTapInfo;

/// dust_dds patch of `tms::PowerPortConversionInfo` — `@nested @extensibility(APPENDABLE)` (idl:4202),
/// member-only (`PassiveConversionInfo.port_conversion`). `conversion_taps` is a
/// `sequence<ConversionTapInfo>` — a non-primitive-element sequence carrying the rule-(12) NEXTINT
/// delimiter (vendored dust_dds patch; `vendor/dust_dds/PATCHES.md`). Field order = the IDL member
/// order, matching `lib_tms::generated::PowerPortConversionInfo`. Derive-only: no hand impl, so this
/// is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct PowerPortConversionInfo {
    pub port_number: i32,
    pub conversion_taps: Vec<ConversionTapInfo>,
    pub change_under_load: bool,
}
