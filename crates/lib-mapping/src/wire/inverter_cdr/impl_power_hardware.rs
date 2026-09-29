//! XCDR2 CDR binding for `PowerHardwareUpdate` (inverter sourced) — only
//! `power_electronics.temperature`. CDR twin of `wire::inverter::impl_power_hardware`.

use lib_parse::canonical::CanonicalInverterReading;
use lib_tms::fields::Absent;
use lib_tms::generated_cdr::power_electronics_state_cdr::PowerElectronicsStateCdrSrc;
use lib_tms::generated_cdr::power_hardware_update_cdr::PowerHardwareUpdateCdrSrc;
use lib_tms::{ClockMonotonic, Identity};

/// Power electronics state — only temperature from the inverter (°C, unscaled).
pub struct PowerElectronicsInverterCdrView {
    temperature_c: i16,
}

impl PowerElectronicsStateCdrSrc for PowerElectronicsInverterCdrView {
    fn temperature(&self) -> f32 {
        self.temperature_c as f32
    }
}

/// `PowerHardwareUpdate` inverter CDR view — power_electronics only, all other subsystems absent.
pub struct PowerHardwareUpdateInverterCdrView<'a> {
    pub reading: &'a CanonicalInverterReading,
    pub device_id: &'a Identity,
    pub now: &'a ClockMonotonic,
}

impl<'a> PowerHardwareUpdateCdrSrc for PowerHardwareUpdateInverterCdrView<'a> {
    fn device_id(&self) -> &str {
        self.device_id.as_str()
    }
    type Timestamp<'b>
        = &'b ClockMonotonic
    where
        Self: 'b;
    fn timestamp(&self) -> Self::Timestamp<'_> {
        self.now
    }
    type Engine<'b>
        = Absent
    where
        Self: 'b;
    fn engine(&self) -> Option<Self::Engine<'_>> {
        None
    }
    type Fuel<'b>
        = Absent
    where
        Self: 'b;
    fn fuel(&self) -> Option<Self::Fuel<'_>> {
        None
    }
    type Generator<'b>
        = Absent
    where
        Self: 'b;
    fn generator(&self) -> Option<Self::Generator<'_>> {
        None
    }
    type EnergyStorage<'b>
        = Absent
    where
        Self: 'b;
    fn energy_storage(&self) -> Option<Self::EnergyStorage<'_>> {
        None
    }
    type PowerElectronics<'b>
        = PowerElectronicsInverterCdrView
    where
        Self: 'b;
    fn power_electronics(&self) -> Option<Self::PowerElectronics<'_>> {
        Some(PowerElectronicsInverterCdrView {
            temperature_c: self.reading.inverter_temperature_c,
        })
    }
    type Thermal<'b>
        = Absent
    where
        Self: 'b;
    fn thermal(&self) -> Option<Self::Thermal<'_>> {
        None
    }
}
