use dust_dds::infrastructure::type_support::TypeSupport;

use super::curve2d::Curve2D;

/// dust_dds patch of `tms::LoadStepResponse` — `@nested @extensibility(APPENDABLE)` (idl:1953),
/// member-only (`LoadSharingInfo.load_response`). Four required `float` scalars followed by eight
/// required `Curve2D` acceptance/rejection curves. `Curve2D` is the pre-existing shared patch type
/// (`super::curve2d`). Field order = the IDL member order, matching
/// `lib_tms::generated::LoadStepResponse`. Derive-only: no hand impl, so this is a single-type file
/// by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct LoadStepResponse {
    pub load_step_low_real_power: f32,
    pub load_step_high_real_power: f32,
    pub load_step_low_reactive_power: f32,
    pub load_step_high_reactive_power: f32,
    pub load_acceptance_frequency: Curve2D,
    pub load_acceptance_voltage: Curve2D,
    pub load_acceptance_real_power: Curve2D,
    pub load_acceptance_reactive_power: Curve2D,
    pub load_rejection_frequency: Curve2D,
    pub load_rejection_voltage: Curve2D,
    pub load_rejection_real_power: Curve2D,
    pub load_rejection_reactive_power: Curve2D,
}
