use dust_dds::infrastructure::type_support::TypeSupport;

use super::energy_start_stop_level::EnergyStartStopLevel;
use super::power_switch_reason::PowerSwitchReason;
use super::power_switch_transition_code::PowerSwitchTransitionCode;

/// dust_dds patch of `tms::EnergyStartStopState` (topic `EnergyStartStopState`) — `@topic
/// @extensibility(APPENDABLE)` (idl:4476). `device_id` is the instance `@key`.
/// `present_level_actor` is `@optional string` → `Option<String>` + `#[dust_dds(optional)]`
/// (wp-derive; the XCDR2 serializer emits a 1-byte presence flag before it in declared order,
/// wp-serializer; the deserializer mirrors it, wp-deserializer). All other members are required.
/// Field order = the IDL member order, matching `lib_tms::generated::EnergyStartStopState`.
/// Derive-only: no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(extensibility = "appendable")]
pub struct EnergyStartStopState {
    #[dust_dds(key)]
    pub device_id: String,
    pub present_level: EnergyStartStopLevel,
    pub future_level: EnergyStartStopLevel,
    pub request_lock: bool,
    pub elapsed_time: f32,
    pub remaining_time: f32,
    pub present_level_reason: PowerSwitchReason,
    #[dust_dds(optional)]
    pub present_level_actor: Option<String>,
    pub future_level_reason: PowerSwitchTransitionCode,
}
