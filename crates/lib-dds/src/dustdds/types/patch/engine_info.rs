use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::EngineInfo` — `@nested @extensibility(APPENDABLE)` (idl:1729), member-only
/// (`PowerHardwareInfo.engine`). All eight members are required `float` limits (NO `@optional` — the
/// `wet_stack`/`fuel_consumption_rate` optionals live on `EngineState`, a different topic's type).
/// Field order = the IDL member order, matching `lib_tms::generated::EngineInfo`. Derive-only: no
/// hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct EngineInfo {
    pub min_oil_pressure: f32,
    pub max_oil_pressure: f32,
    pub min_coolant_temperature: f32,
    pub max_coolant_temperature: f32,
    pub min_engine_speed: f32,
    pub max_engine_speed: f32,
    pub min_wet_stack_prevention_load: f32,
    pub min_wet_stack_mitigation_load: f32,
}
