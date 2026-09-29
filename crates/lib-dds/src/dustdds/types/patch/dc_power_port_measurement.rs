use dust_dds::infrastructure::type_support::TypeSupport;

use super::dc_power_line_measurement::DcPowerLineMeasurement;

/// dust_dds patch of `tms::dc::PowerPortMeasurement` — `{ long port_number; sequence<PowerLine...>
/// line; float power_rate_of_change; }`, member-only (nested inside `DcMeasurementUpdate`'s
/// `external_measurement`/`internal_measurement` sequences). `line` is `Vec<T>` (not
/// `heapless::Vec`) — dust_dds's `XTypesBinding` blanket only covers `alloc::vec::Vec<T: TypeSupport>`
/// (this crate is Pi-only std, so the heapless bound is a `lib_tms`-internal concern, not a wire
/// one). Field order = the IDL member order. Derive-only: no hand impl, so this is a single-type
/// file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct DcPowerPortMeasurement {
    pub port_number: i32,
    pub line: Vec<DcPowerLineMeasurement>,
    pub power_rate_of_change: f32,
}
