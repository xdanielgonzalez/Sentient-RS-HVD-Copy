use dust_dds::infrastructure::type_support::TypeSupport;

use super::curve2d::Curve2D;
use super::load_step_response::LoadStepResponse;

/// dust_dds patch of `tms::LoadSharingInfo` — `@nested @extensibility(APPENDABLE)` (idl:2057),
/// member-only (`SourceInfo`/`StorageInfo.load_sharing`, `ActiveConversionInfo.load_sharing`).
/// `max_overload_real_power` is `@optional Curve2D` → `Option<Curve2D>` + `#[dust_dds(optional)]`;
/// `load_response` is `@optional sequence<LoadStepResponse>` → `Option<Vec<LoadStepResponse>>` +
/// `#[dust_dds(optional)]` (the chain uses `emit_optional_seq` → boolean-then-sequence; the inner
/// non-primitive-element sequence still carries the rule-(12) NEXTINT delimiter when present).
/// Field order = the IDL member order, matching `lib_tms::generated::LoadSharingInfo`. Derive-only:
/// no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct LoadSharingInfo {
    pub port_number: i32,
    pub supports_droop: bool,
    pub supports_multi_segment_droop: bool,
    pub supports_constant_power: bool,
    pub min_real_power: f32,
    pub max_real_power: f32,
    #[dust_dds(optional)]
    pub max_overload_real_power: Option<Curve2D>,
    pub min_reactive_power: f32,
    pub max_reactive_power: f32,
    pub max_apparent_power: f32,
    pub power_factor: f32,
    #[dust_dds(optional)]
    pub load_response: Option<Vec<LoadStepResponse>>,
}
