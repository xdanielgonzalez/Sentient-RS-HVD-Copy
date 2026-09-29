use dust_dds::infrastructure::type_support::TypeSupport;

use super::curve2d::Curve2D;

/// dust_dds patch of `tms::StorageUpdate` (topic `StorageUpdate`) — `@topic
/// @extensibility(APPENDABLE)` (idl:5113). `device_id` is the instance `@key`. Three members are
/// `@optional float` → `Option<f32>` + `#[dust_dds(optional)]` (wp-derive; the XCDR2 serializer emits
/// a 1-byte presence flag before each in declared order, wp-serializer; the deserializer mirrors it,
/// wp-deserializer): `state_of_charge`, `max_charge_rate`, `max_discharge_rate` (the latter two are
/// the trailing members, `state_of_charge` sits between required scalars — so the presence flags are
/// non-contiguous, each emitted at its declared position). The four `Curve2D` members are required
/// non-primitive members (each a `sequence<Point2D>` carrying the rule-(12) NEXTINT delimiter).
/// Field order = the IDL member order, matching `lib_tms::generated::StorageUpdate`. Derive-only: no
/// hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(extensibility = "appendable")]
pub struct StorageUpdate {
    #[dust_dds(key)]
    pub device_id: String,
    pub internal_voltage: f32,
    #[dust_dds(optional)]
    pub state_of_charge: Option<f32>,
    pub available_energy: f32,
    pub hold_time: f32,
    pub charge_time: Curve2D,
    pub discharge_time: Curve2D,
    pub max_charge_time: Curve2D,
    pub max_discharge_time: Curve2D,
    #[dust_dds(optional)]
    pub max_charge_rate: Option<f32>,
    #[dust_dds(optional)]
    pub max_discharge_rate: Option<f32>,
}
