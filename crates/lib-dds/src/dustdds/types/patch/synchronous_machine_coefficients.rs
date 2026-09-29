use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::SynchronousMachineCoefficients` — `@nested @extensibility(APPENDABLE)`
/// (idl:1284), member-only (`PowerHardwareInfo.synchronous_machine_model`). Twenty required `float`
/// coefficients. Field order = the IDL member order, matching
/// `lib_tms::generated::SynchronousMachineCoefficients`. Derive-only: no hand impl, so this is a
/// single-type file by design.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct SynchronousMachineCoefficients {
    pub stator_resistance_per_phase: f32,
    pub stator_leakage_reactance: f32,
    pub zero_sequence_reactance: f32,
    pub negative_sequence_reactance: f32,
    pub zero_sequence_resistance: f32,
    pub negative_sequence_resistance: f32,
    pub direct_axis_synchronous_reactance: f32,
    pub direct_axis_transient_reactance: f32,
    pub direct_axis_subtransient_reactance: f32,
    pub quadrature_axis_synchronous_reactance: f32,
    pub quadrature_axis_transient_reactance: f32,
    pub quadrature_axis_subtransient_reactance: f32,
    pub direct_axis_transient_short_circuit_time_constant: f32,
    pub direct_axis_subtransient_short_circuit_time_constant: f32,
    pub direct_axis_transient_open_circuit_time_constant: f32,
    pub direct_axis_subtransient_open_circuit_time_constant: f32,
    pub quadrature_axis_transient_short_circuit_time_constant: f32,
    pub quadrature_axis_subtransient_short_circuit_time_constant: f32,
    pub quadrature_axis_transient_open_circuit_time_constant: f32,
    pub quadrature_axis_subtransient_open_circuit_time_constant: f32,
}
