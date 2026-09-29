//! XCDR2 CDR binding for `DcMeasurementUpdate` (inverter sourced) — PV @ port 2, battery @ port 3.
//! CDR twin of `wire::inverter::impl_dc_measurement`; values UNSCALED (real `f32`).

use core::iter;
use lib_parse::canonical::CanonicalInverterReading;
use lib_tms::fields::Absent;
use lib_tms::generated_cdr::dc_measurement_update_cdr::DcMeasurementUpdateCdrSrc;
use lib_tms::generated_cdr::dc_power_line_measurement_cdr::DcPowerLineMeasurementCdrSrc;
use lib_tms::generated_cdr::dc_power_port_measurement_cdr::DcPowerPortMeasurementCdrSrc;
use lib_tms::{ClockMonotonic, Identity};

/// DC power line — voltage (cV -> V), amperage (source-positive, A), power (direct or V×A).
#[derive(Clone)]
pub struct DcLineInverterCdrView {
    voltage_cv: u16,
    amperage_source_positive: f32,
    power_w: Option<u16>, // Some(direct) for PV, None -> calculated for battery
}

impl DcPowerLineMeasurementCdrSrc for DcLineInverterCdrView {
    fn voltage(&self) -> f32 {
        self.voltage_cv as f32 / 100.0
    }
    fn amperage(&self) -> f32 {
        self.amperage_source_positive
    }
    fn power(&self) -> f32 {
        match self.power_w {
            Some(w) => w as f32,
            None => (self.voltage_cv as f32 / 100.0) * self.amperage_source_positive,
        }
    }
}

/// DC power port (inverter sourced).
pub struct DcPortInverterCdrView {
    port_number: i32,
    line: DcLineInverterCdrView,
}

impl DcPowerPortMeasurementCdrSrc for DcPortInverterCdrView {
    fn port_number(&self) -> i32 {
        self.port_number
    }
    type LineItem<'b>
        = DcLineInverterCdrView
    where
        Self: 'b;
    fn line(&self) -> impl Iterator<Item = Self::LineItem<'_>> + '_ {
        iter::once(self.line.clone())
    }
    fn power_rate_of_change(&self) -> f32 {
        f32::NAN
    }
}

/// `DcMeasurementUpdate` inverter CDR view — PV @ port 2, battery @ port 3.
pub struct DcMeasurementUpdateInverterCdrView<'a> {
    pub reading: &'a CanonicalInverterReading,
    pub device_id: &'a Identity,
    pub now: &'a ClockMonotonic,
}

impl<'a> DcMeasurementUpdateCdrSrc for DcMeasurementUpdateInverterCdrView<'a> {
    fn device_id(&self) -> &str {
        self.device_id.as_str()
    }
    type TimeMeasured<'b>
        = &'b ClockMonotonic
    where
        Self: 'b;
    fn time_measured(&self) -> Self::TimeMeasured<'_> {
        self.now
    }
    type ExternalMeasurementItem<'b>
        = DcPortInverterCdrView
    where
        Self: 'b;
    fn external_measurement(&self) -> impl Iterator<Item = Self::ExternalMeasurementItem<'_>> + '_ {
        let r = self.reading;
        // PV port: voltage (dV -> stored cV), current (dA -> A), power (W direct).
        let pv_port = DcPortInverterCdrView {
            port_number: 2,
            line: DcLineInverterCdrView {
                voltage_cv: (r.pv_voltage_dv as f32 * 10.0) as u16,
                amperage_source_positive: r.pv_current_da as f32 / 10.0,
                power_w: Some(r.pv_charging_power_w),
            },
        };
        // Battery port: voltage (cV), amperage (source-positive: discharge - charge), power = V×A.
        let battery_amperage =
            r.battery_discharge_current_a as f32 - r.battery_charging_current_a as f32;
        let battery_port = DcPortInverterCdrView {
            port_number: 3,
            line: DcLineInverterCdrView {
                voltage_cv: r.battery_voltage_cv,
                amperage_source_positive: battery_amperage,
                power_w: None,
            },
        };
        iter::once(pv_port).chain(iter::once(battery_port))
    }
    type InternalMeasurementItem<'b>
        = Absent
    where
        Self: 'b;
    fn internal_measurement(&self) -> impl Iterator<Item = Self::InternalMeasurementItem<'_>> + '_ {
        iter::empty()
    }
}
