use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::dc::PowerLineMeasurement` — `{ float voltage; float amperage; float
/// power; }`, member-only (nested inside `DcPowerPortMeasurement.line`). Field order = the IDL
/// member order. Derive-only: no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct DcPowerLineMeasurement {
    pub voltage: f32,
    pub amperage: f32,
    pub power: f32,
}
