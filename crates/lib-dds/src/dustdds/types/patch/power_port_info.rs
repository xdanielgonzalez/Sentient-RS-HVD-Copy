use dust_dds::infrastructure::type_support::TypeSupport;

use super::circuit_wiring::CircuitWiring;
use super::power_connector_info::PowerConnectorInfo;
use super::power_port_directionality::PowerPortDirectionality;
use super::power_switch_info::PowerSwitchInfo;

/// dust_dds patch of `tms::PowerPortInfo` — `@nested @extensibility(APPENDABLE)` (idl:2687),
/// member-only (`PowerDeviceInfo.power_ports`). `min_frequency`/`max_frequency` are `@optional float`
/// and `switch_info` is `@optional PowerSwitchInfo` → `Option<T>` + `#[dust_dds(optional)]` (wp-derive;
/// XCDR2 presence flag from wp-serializer). Field order = the IDL member order, matching
/// `lib_tms::generated::PowerPortInfo`. Derive-only: no hand impl, so this is a single-type file by
/// design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct PowerPortInfo {
    pub port_number: i32,
    pub wiring: CircuitWiring,
    pub directionality: PowerPortDirectionality,
    pub has_switch: bool,
    pub has_external_meter: bool,
    pub has_internal_meter: bool,
    pub has_summary_measurement_update: bool,
    pub min_amperage: f32,
    pub max_amperage: f32,
    pub short_circuit_amperage: f32,
    pub min_voltage: f32,
    pub max_voltage: f32,
    #[dust_dds(optional)]
    pub min_frequency: Option<f32>,
    #[dust_dds(optional)]
    pub max_frequency: Option<f32>,
    pub connector_info: PowerConnectorInfo,
    #[dust_dds(optional)]
    pub switch_info: Option<PowerSwitchInfo>,
}
