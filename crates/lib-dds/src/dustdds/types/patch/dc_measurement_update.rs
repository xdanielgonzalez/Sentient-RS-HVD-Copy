use dust_dds::infrastructure::type_support::TypeSupport;

use super::clock_monotonic::ClockMonotonic;
use super::dc_power_port_measurement::DcPowerPortMeasurement;

/// dust_dds patch of `tms::dc::MeasurementUpdate` (topics `DcMeasurementUpdate` /
/// `DcSummaryMeasurementUpdate`) — `{ string device_id @key; ClockMonotonic time_measured;
/// sequence<PowerPortMeasurement> external_measurement; sequence<PowerPortMeasurement>
/// internal_measurement; }`. Every member is required (no `@optional` anywhere in this topic's
/// transitive closure), so the WP4 rung-C round-trip is byte-identical (the WP3 cross-oracle already
/// proves it for this exact shape). Field order = the IDL member order (the chains' order).
/// Derive-only: no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(extensibility = "appendable")]
pub struct DcMeasurementUpdate {
    #[dust_dds(key)]
    pub device_id: String,
    pub time_measured: ClockMonotonic,
    pub external_measurement: Vec<DcPowerPortMeasurement>,
    pub internal_measurement: Vec<DcPowerPortMeasurement>,
}
