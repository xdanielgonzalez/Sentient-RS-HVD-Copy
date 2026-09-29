//! XCDR2 CDR binding for `AcMeasurementUpdate` (inverter sourced) — the CDR twin of
//! `wire::inverter::impl_ac_measurement`. Value math ported verbatim, but UNSCALED (real `f32`,
//! no `FixedNum`); a required field with no canonical source emits `f32::NAN` (the CDR analogue of
//! the JSON `null`), an `@optional` field emits absent (presence flag `0x00`).

use core::iter;
use lib_parse::canonical::CanonicalInverterReading;
use lib_tms::fields::Absent;
use lib_tms::generated_cdr::ac_measurement_update_cdr::AcMeasurementUpdateCdrSrc;
use lib_tms::generated_cdr::ac_power_line_measurement_cdr::AcPowerLineMeasurementCdrSrc;
use lib_tms::generated_cdr::ac_power_port_measurement_cdr::AcPowerPortMeasurementCdrSrc;
use lib_tms::{ClockMonotonic, Identity};

/// AC power line — voltage (dV -> V), frequency (dHz -> Hz), amperage (S/V or NaN), real power.
#[derive(Clone)]
pub struct AcLineInverterCdrView {
    voltage_dv: u16,
    frequency_dhz: u16,
    current_va_over_v: Option<f32>, // S / V if voltage > 0, else None -> NaN
    real_power_w: Option<u16>,      // Some(watts) if available, None -> NaN
}

impl AcPowerLineMeasurementCdrSrc for AcLineInverterCdrView {
    fn voltage(&self) -> f32 {
        self.voltage_dv as f32 / 10.0
    }
    fn frequency(&self) -> f32 {
        self.frequency_dhz as f32 / 10.0
    }
    fn amperage(&self) -> f32 {
        self.current_va_over_v.unwrap_or(f32::NAN)
    }
    fn phase_offset(&self) -> Option<f32> {
        None
    }
    fn real_power(&self) -> f32 {
        self.real_power_w.map(|w| w as f32).unwrap_or(f32::NAN)
    }
    fn reactive_power(&self) -> f32 {
        f32::NAN
    }
}

/// AC power port — wraps one AC line with port_number + rate-of-change fields.
pub struct AcPortInverterCdrView {
    port_number: i32,
    line: AcLineInverterCdrView,
}

impl AcPowerPortMeasurementCdrSrc for AcPortInverterCdrView {
    fn port_number(&self) -> i32 {
        self.port_number
    }
    type LineItem<'b>
        = AcLineInverterCdrView
    where
        Self: 'b;
    fn line(&self) -> impl Iterator<Item = Self::LineItem<'_>> + '_ {
        iter::once(self.line.clone())
    }
    fn phase_angle(&self) -> &[f32] {
        &[]
    }
    fn real_power_rate_of_change(&self) -> f32 {
        f32::NAN
    }
    fn reactive_power_rate_of_change(&self) -> f32 {
        f32::NAN
    }
}

/// `AcMeasurementUpdate` inverter CDR view — AC output @ port 0, grid input @ port 1.
pub struct AcMeasurementUpdateInverterCdrView<'a> {
    pub reading: &'a CanonicalInverterReading,
    pub device_id: &'a Identity,
    pub now: &'a ClockMonotonic,
}

impl<'a> AcMeasurementUpdateCdrSrc for AcMeasurementUpdateInverterCdrView<'a> {
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
        = AcPortInverterCdrView
    where
        Self: 'b;
    fn external_measurement(&self) -> impl Iterator<Item = Self::ExternalMeasurementItem<'_>> + '_ {
        let r = self.reading;
        // AC output line: voltage, frequency, amperage (S / V if voltage > 0, else NaN), power.
        let output_voltage = r.ac_output_voltage_dv as f32 / 10.0;
        let output_amperage = if r.ac_output_voltage_dv > 0 {
            Some(r.ac_output_apparent_power_va as f32 / output_voltage)
        } else {
            None
        };
        let output_port = AcPortInverterCdrView {
            port_number: 0,
            line: AcLineInverterCdrView {
                voltage_dv: r.ac_output_voltage_dv,
                frequency_dhz: r.ac_output_frequency_dhz,
                current_va_over_v: output_amperage,
                real_power_w: Some(r.ac_output_active_power_w),
            },
        };
        // Grid input line: voltage, frequency, NaN amperage/power (not in QPIGS).
        let grid_port = AcPortInverterCdrView {
            port_number: 1,
            line: AcLineInverterCdrView {
                voltage_dv: r.grid_voltage_dv,
                frequency_dhz: r.grid_frequency_dhz,
                current_va_over_v: None,
                real_power_w: None,
            },
        };
        iter::once(output_port).chain(iter::once(grid_port))
    }
    type InternalMeasurementItem<'b>
        = Absent
    where
        Self: 'b;
    fn internal_measurement(&self) -> impl Iterator<Item = Self::InternalMeasurementItem<'_>> + '_ {
        iter::empty()
    }
}
