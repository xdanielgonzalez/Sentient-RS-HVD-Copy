//! `WireFlat` impl for `AcMeasurementUpdate` — delegates to the generated field chain.

use crate::wire::{JsonSink, WireError, WireFlat};
use core::iter;
use lib_parse::canonical::CanonicalInverterReading;
use lib_tms::fields::{emit_object, FloatVal};
use lib_tms::generated_fields::ac_measurement_update_fields::AcMeasurementUpdateSrc;
use lib_tms::generated_fields::ac_power_line_measurement_fields::AcPowerLineMeasurementSrc;
use lib_tms::generated_fields::ac_power_port_measurement_fields::AcPowerPortMeasurementSrc;
use lib_tms::{ClockMonotonic, Identity};

/// Flat wire binding for `AcMeasurementUpdate` — borrows the canonical inverter reading + device
/// context; serializes byte-identically to `transform_inverter::to_ac_measurement` via the
/// generated field chains.
pub struct AcMeasurementUpdateWire<'a> {
    pub reading: &'a CanonicalInverterReading,
    pub device_id: &'a Identity,
    pub now: ClockMonotonic,
}

/// AC power line measurement — voltage/frequency/amperage/power components of one AC port's line.
#[derive(Clone)]
struct AcLineView {
    voltage_dv: u16,
    frequency_dhz: u16,
    current_va_over_v: Option<f32>, // S / V if voltage > 0, else None for NaN
    real_power_w: Option<u16>,      // Some(watts) if available, None for NaN
}

impl AcPowerLineMeasurementSrc for AcLineView {
    fn voltage(&self) -> FloatVal {
        FloatVal::Fixed(lib_tms::fields::FixedNum {
            scaled: self.voltage_dv as i64,
            frac_digits: 1,
            invert_sign: false,
        })
    }
    fn frequency(&self) -> FloatVal {
        FloatVal::Fixed(lib_tms::fields::FixedNum {
            scaled: self.frequency_dhz as i64,
            frac_digits: 1,
            invert_sign: false,
        })
    }
    fn amperage(&self) -> FloatVal {
        match self.current_va_over_v {
            Some(a) => FloatVal::F32(a),
            None => FloatVal::Null,
        }
    }
    fn phase_offset(&self) -> Option<FloatVal> {
        None
    }
    fn real_power(&self) -> FloatVal {
        match self.real_power_w {
            Some(w) => FloatVal::F32(w as f32),
            None => FloatVal::Null,
        }
    }
    fn reactive_power(&self) -> FloatVal {
        FloatVal::Null
    }
}

/// AC power port — wraps an AC line with port_number and rate-of-change fields.
struct AcPortView {
    port_number: i32,
    line: AcLineView,
}

impl AcPowerPortMeasurementSrc for AcPortView {
    fn port_number(&self) -> i64 {
        self.port_number as i64
    }
    type LineItem<'b>
        = AcLineView
    where
        Self: 'b;
    fn line(&self) -> impl Iterator<Item = Self::LineItem<'_>> + '_ {
        iter::once(self.line.clone())
    }
    fn phase_angle(&self) -> &[f32] {
        &[]
    }
    fn real_power_rate_of_change(&self) -> FloatVal {
        FloatVal::Null
    }
    fn reactive_power_rate_of_change(&self) -> FloatVal {
        FloatVal::Null
    }
}

/// AC measurement update wrapper — the two ports (AC output @ port 0, grid input @ port 1).
struct AcMeasurementView<'a> {
    reading: &'a CanonicalInverterReading,
    device_id: &'a Identity,
    now: &'a ClockMonotonic,
}

impl<'a> AcMeasurementUpdateSrc for AcMeasurementView<'a> {
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
        = AcPortView
    where
        Self: 'b;
    fn external_measurement(&self) -> impl Iterator<Item = Self::ExternalMeasurementItem<'_>> + '_ {
        let r = self.reading;
        // AC output line: voltage, frequency, amperage (S / V if voltage > 0, else NaN), power
        let output_voltage = r.ac_output_voltage_dv as f32 / 10.0;
        let output_amperage = if r.ac_output_voltage_dv > 0 {
            Some(r.ac_output_apparent_power_va as f32 / output_voltage)
        } else {
            None
        };
        let output_line = AcLineView {
            voltage_dv: r.ac_output_voltage_dv,
            frequency_dhz: r.ac_output_frequency_dhz,
            current_va_over_v: output_amperage,
            real_power_w: Some(r.ac_output_active_power_w),
        };
        let output_port = AcPortView {
            port_number: 0,
            line: output_line,
        };
        // Grid input line: voltage, frequency, NaN amperage/power (not available in QPIGS)
        let grid_line = AcLineView {
            voltage_dv: r.grid_voltage_dv,
            frequency_dhz: r.grid_frequency_dhz,
            current_va_over_v: None, // NaN amperage
            real_power_w: None,      // NaN power
        };
        let grid_port = AcPortView {
            port_number: 1,
            line: grid_line,
        };
        iter::once(output_port).chain(iter::once(grid_port))
    }
    type InternalMeasurementItem<'b>
        = lib_tms::fields::Absent
    where
        Self: 'b;
    fn internal_measurement(&self) -> impl Iterator<Item = Self::InternalMeasurementItem<'_>> + '_ {
        iter::empty()
    }
}

impl WireFlat for AcMeasurementUpdateWire<'_> {
    fn serialize_flat(&self, sink: &mut impl JsonSink) -> Result<(), WireError> {
        let view = AcMeasurementView {
            reading: self.reading,
            device_id: self.device_id,
            now: &self.now,
        };
        emit_object::<_, lib_tms::generated_fields::ac_measurement_update_fields::Chain, _>(
            &view, sink,
        );
        Ok(())
    }
}
