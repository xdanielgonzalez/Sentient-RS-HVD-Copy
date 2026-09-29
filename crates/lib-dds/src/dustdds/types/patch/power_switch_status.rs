use dust_dds::infrastructure::type_support::TypeSupport;

use super::circuit_continuity::CircuitContinuity;
use super::power_switch_reason::PowerSwitchReason;
use super::power_switch_transition_code::PowerSwitchTransitionCode;

/// dust_dds patch of `tms::PowerSwitchStatus` — `@nested @extensibility(APPENDABLE)` (idl:2829),
/// member-only (`PowerPortStatus.switch_status`, itself `@optional`). `present_continuity_actor` is
/// `@optional string` → `Option<String>` + `#[dust_dds(optional)]` (wp-derive; the XCDR2 serializer
/// emits a 1-byte presence flag before it in declared order, wp-serializer; the deserializer mirrors
/// it, wp-deserializer). The other members are required. Field order = the IDL member order, matching
/// `lib_tms::generated::PowerSwitchStatus`. Derive-only: no hand impl, so this is a single-type file
/// by design.
///
/// Exclusive to the `PowerPortState` closure (via `PowerPortStatus`); authored here by
/// wp-topic-small5.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct PowerSwitchStatus {
    pub present_continuity: CircuitContinuity,
    pub future_continuity: CircuitContinuity,
    pub request_lock: bool,
    pub present_continuity_reason: PowerSwitchReason,
    #[dust_dds(optional)]
    pub present_continuity_actor: Option<String>,
    pub future_continuity_reason: PowerSwitchTransitionCode,
}
