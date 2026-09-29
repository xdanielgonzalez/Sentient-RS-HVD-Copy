//! `WireFlat` impl for `PowerHardwareUpdate` (inverter sourced) — only power_electronics.temperature.

use crate::wire::{JsonSink, WireError, WireFlat};
use lib_parse::canonical::CanonicalInverterReading;
use lib_tms::fields::{emit_object, FloatVal};
use lib_tms::generated_fields::power_electronics_state_fields::PowerElectronicsStateSrc;
use lib_tms::generated_fields::power_hardware_update_fields::PowerHardwareUpdateSrc;
use lib_tms::{ClockMonotonic, Identity};

/// Flat wire binding for `PowerHardwareUpdate` (inverter sourced) — only
/// `power_electronics.temperature`.
pub struct PowerHardwareUpdateInverterWire<'a> {
    pub reading: &'a CanonicalInverterReading,
    pub device_id: &'a Identity,
    pub now: ClockMonotonic,
}

/// Power electronics state — only temperature from inverter.
struct PowerElectronicsInverterView {
    temperature_c: i16,
}

impl PowerElectronicsStateSrc for PowerElectronicsInverterView {
    fn temperature(&self) -> FloatVal {
        FloatVal::F32(self.temperature_c as f32)
    }
}

/// Power hardware update wrapper (inverter sourced).
struct PowerHardwareInverterView<'a> {
    reading: &'a CanonicalInverterReading,
    device_id: &'a Identity,
    now: &'a ClockMonotonic,
}

impl<'a> PowerHardwareUpdateSrc for PowerHardwareInverterView<'a> {
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
        = lib_tms::fields::Absent
    where
        Self: 'b;
    fn engine(&self) -> Option<Self::Engine<'_>> {
        None
    }
    type Fuel<'b>
        = lib_tms::fields::Absent
    where
        Self: 'b;
    fn fuel(&self) -> Option<Self::Fuel<'_>> {
        None
    }
    type Generator<'b>
        = lib_tms::fields::Absent
    where
        Self: 'b;
    fn generator(&self) -> Option<Self::Generator<'_>> {
        None
    }
    type EnergyStorage<'b>
        = lib_tms::fields::Absent
    where
        Self: 'b;
    fn energy_storage(&self) -> Option<Self::EnergyStorage<'_>> {
        None
    }
    type PowerElectronics<'b>
        = PowerElectronicsInverterView
    where
        Self: 'b;
    fn power_electronics(&self) -> Option<Self::PowerElectronics<'_>> {
        Some(PowerElectronicsInverterView {
            temperature_c: self.reading.inverter_temperature_c,
        })
    }
    type Thermal<'b>
        = lib_tms::fields::Absent
    where
        Self: 'b;
    fn thermal(&self) -> Option<Self::Thermal<'_>> {
        None
    }
}

impl WireFlat for PowerHardwareUpdateInverterWire<'_> {
    fn serialize_flat(&self, sink: &mut impl JsonSink) -> Result<(), WireError> {
        let view = PowerHardwareInverterView {
            reading: self.reading,
            device_id: self.device_id,
            now: &self.now,
        };
        emit_object::<_, lib_tms::generated_fields::power_hardware_update_fields::Chain, _>(
            &view, sink,
        );
        Ok(())
    }
}
