use dust_dds::infrastructure::type_support::TypeSupport;

use super::cable_sense_status::CableSenseStatus;
use super::power_port_id::PowerPortId;
use super::power_switch_status::PowerSwitchStatus;

/// dust_dds patch of `tms::PowerPortStatus` — `@nested @extensibility(APPENDABLE)` (idl:3074),
/// member-only (`PowerPortState.power_ports`, a `sequence<PowerPortStatus>`). Three members are
/// `@optional` → `Option<T>` + `#[dust_dds(optional)]` (wp-derive; the XCDR2 serializer emits a
/// 1-byte presence flag before each in declared order, wp-serializer; the deserializer mirrors it,
/// wp-deserializer): `cable_id` (`Option<String>`), `connection` (`Option<PowerPortId>`),
/// `switch_status` (`Option<PowerSwitchStatus>`). `port_number` and `cable_status` are required.
/// Field order = the IDL member order, matching `lib_tms::generated::PowerPortStatus`. Derive-only:
/// no hand impl, so this is a single-type file by design.
///
/// Exclusive to the `PowerPortState` closure; authored here by wp-topic-small5.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct PowerPortStatus {
    pub port_number: i32,
    pub cable_status: CableSenseStatus,
    #[dust_dds(optional)]
    pub cable_id: Option<String>,
    #[dust_dds(optional)]
    pub connection: Option<PowerPortId>,
    #[dust_dds(optional)]
    pub switch_status: Option<PowerSwitchStatus>,
}
