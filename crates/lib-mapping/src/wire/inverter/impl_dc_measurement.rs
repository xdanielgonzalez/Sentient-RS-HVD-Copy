//! `WireFlat` impl for `DcMeasurementUpdate` (inverter sourced) — PV + battery ports.

use crate::wire::{JsonSink, WireError, WireFlat};
use core::iter;
use lib_parse::canonical::CanonicalInverterReading;
use lib_tms::fields::{emit_object, FloatVal};
use lib_tms::generated_fields::dc_measurement_update_fields::DcMeasurementUpdateSrc;
use lib_tms::generated_fields::dc_power_line_measurement_fields::DcPowerLineMeasurementSrc;
use lib_tms::generated_fields::dc_power_port_measurement_fields::DcPowerPortMeasurementSrc;
use lib_tms::{ClockMonotonic, Identity};

/// Flat wire binding for `DcMeasurementUpdate` (inverter sourced) — PV @ port 2, battery @ port 3.
pub struct DcMeasurementUpdateInverterWire<'a> {
    pub reading: &'a CanonicalInverterReading,
    pub device_id: &'a Identity,
    pub now: ClockMonotonic,
}

/// DC power line — voltage, current (source-positive), power.
/// Power is either direct (from device) or calculated from V×A.
#[derive(Clone)]
struct DcLineInverterView {
    voltage_cv: u16,
    amperage_source_positive: f32,
    power_w: Option<u16>, // Some for direct (PV), None for calculated (battery)
}

impl DcPowerLineMeasurementSrc for DcLineInverterView {
    fn voltage(&self) -> FloatVal {
        FloatVal::Fixed(lib_tms::fields::FixedNum {
            scaled: self.voltage_cv as i64,
            frac_digits: 2,
            invert_sign: false,
        })
    }
    fn amperage(&self) -> FloatVal {
        FloatVal::F32(self.amperage_source_positive)
    }
    fn power(&self) -> FloatVal {
        match self.power_w {
            Some(w) => FloatVal::F32(w as f32),
            None => {
                // Calculated: V × A
                let v = self.voltage_cv as f32 / 100.0;
                FloatVal::F32(v * self.amperage_source_positive)
            }
        }
    }
}

/// DC power port (inverter sourced).
struct DcPortInverterView {
    port_number: i32,
    line: DcLineInverterView,
}

impl DcPowerPortMeasurementSrc for DcPortInverterView {
    fn port_number(&self) -> i64 {
        self.port_number as i64
    }
    type LineItem<'b>
        = DcLineInverterView
    where
        Self: 'b;
    fn line(&self) -> impl Iterator<Item = Self::LineItem<'_>> + '_ {
        iter::once(self.line.clone())
    }
    fn power_rate_of_change(&self) -> FloatVal {
        FloatVal::Null
    }
}

/// DC measurement update wrapper (inverter sourced).
struct DcMeasurementInverterView<'a> {
    reading: &'a CanonicalInverterReading,
    device_id: &'a Identity,
    now: &'a ClockMonotonic,
}

impl<'a> DcMeasurementUpdateSrc for DcMeasurementInverterView<'a> {
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
        = DcPortInverterView
    where
        Self: 'b;
    fn external_measurement(&self) -> impl Iterator<Item = Self::ExternalMeasurementItem<'_>> + '_ {
        let r = self.reading;
        // PV port: voltage (dv -> cv), current (da -> A), power (W)
        let pv_line = DcLineInverterView {
            voltage_cv: (r.pv_voltage_dv as f32 * 10.0) as u16, // convert dv to cv for FixedNum
            amperage_source_positive: r.pv_current_da as f32 / 10.0,
            power_w: Some(r.pv_charging_power_w),
        };
        let pv_port = DcPortInverterView {
            port_number: 2,
            line: pv_line,
        };
        // Battery port: voltage (cv), amperage (source-positive: discharge - charge), power calculated
        let battery_amperage =
            r.battery_discharge_current_a as f32 - r.battery_charging_current_a as f32;
        let battery_line = DcLineInverterView {
            voltage_cv: r.battery_voltage_cv,
            amperage_source_positive: battery_amperage,
            power_w: None, // Will be calculated as V × A
        };
        let battery_port = DcPortInverterView {
            port_number: 3,
            line: battery_line,
        };
        iter::once(pv_port).chain(iter::once(battery_port))
    }
    type InternalMeasurementItem<'b>
        = lib_tms::fields::Absent
    where
        Self: 'b;
    fn internal_measurement(&self) -> impl Iterator<Item = Self::InternalMeasurementItem<'_>> + '_ {
        iter::empty()
    }
}

impl WireFlat for DcMeasurementUpdateInverterWire<'_> {
    fn serialize_flat(&self, sink: &mut impl JsonSink) -> Result<(), WireError> {
        let view = DcMeasurementInverterView {
            reading: self.reading,
            device_id: self.device_id,
            now: &self.now,
        };
        emit_object::<_, lib_tms::generated_fields::dc_measurement_update_fields::Chain, _>(
            &view, sink,
        );
        Ok(())
    }
}
