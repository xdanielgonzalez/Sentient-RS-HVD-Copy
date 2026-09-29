use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::ConversionTapInfo` — `@nested @extensibility(APPENDABLE)` (idl:4167),
/// member-only (`PowerPortConversionInfo.conversion_taps`). All members required. Field order = the
/// IDL member order, matching `lib_tms::generated::ConversionTapInfo`. Derive-only: no hand impl, so
/// this is a single-type file by design.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct ConversionTapInfo {
    pub tap_id: i32,
    pub phase_shift: f32,
    pub voltage_ratio: f32,
}
