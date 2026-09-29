use dust_dds::infrastructure::type_support::TypeSupport;

use super::control_curve::ControlCurve;

/// dust_dds patch of `tms::dc::LoadSharingParameters` — `{ long port_number; ControlCurve
/// reference_voltage; float min_amperage; float max_amperage; float min_voltage; float max_voltage;
/// }`, member-only (nested inside `DcLoadSharingState.parameters`). Field order = the IDL member
/// order. Derive-only: no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct DcLoadSharingParameters {
    pub port_number: i32,
    pub reference_voltage: ControlCurve,
    pub min_amperage: f32,
    pub max_amperage: f32,
    pub min_voltage: f32,
    pub max_voltage: f32,
}
