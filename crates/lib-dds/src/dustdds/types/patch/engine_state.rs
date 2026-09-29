use dust_dds::infrastructure::type_support::TypeSupport;

use super::engine_operating_state::EngineOperatingState;

/// dust_dds patch of `tms::EngineState` — `@nested @extensibility(APPENDABLE)` (idl:1831),
/// member-only (`PowerHardwareUpdate.engine`, itself `@optional`). `wet_stack` and
/// `fuel_consumption_rate` are `@optional float` → `Option<f32>` + `#[dust_dds(optional)]` (wp-derive;
/// the XCDR2 serializer emits a 1-byte presence flag before each optional in declared order,
/// wp-serializer). The other members are required. Field order = the IDL member order, matching
/// `lib_tms::generated::EngineState`. Derive-only: no hand impl, so this is a single-type file by
/// design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct EngineState {
    pub operating_state: EngineOperatingState,
    pub oil_pressure: f32,
    pub coolant_temperature: f32,
    pub speed: f32,
    #[dust_dds(optional)]
    pub wet_stack: Option<f32>,
    pub run_time: f32,
    #[dust_dds(optional)]
    pub fuel_consumption_rate: Option<f32>,
}
