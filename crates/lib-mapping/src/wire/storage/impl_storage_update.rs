//! `WireFlat` impl for `StorageUpdate` — battery energy storage state: voltage, SOC, energy, empty curves.

use crate::wire::{JsonSink, WireError, WireFlat};
use lib_parse::canonical::CanonicalReading;
use lib_tms::fields::{emit_object, FloatVal};
use lib_tms::generated_fields::storage_update_fields::StorageUpdateSrc;
use lib_tms::Identity;

/// Flat wire binding for `StorageUpdate` — borrows canonical reading + device context; derives
/// available energy from [`crate::transform::ToStorageUpdate`] math.
pub struct StorageUpdateWire<'a> {
    pub reading: &'a CanonicalReading,
    pub device_id: &'a Identity,
}

/// Storage update wrapper — device_id, voltage, SOC, energy, hold/charge/discharge times (empty curves),
/// rates (None).
struct StorageUpdateView<'a> {
    reading: &'a CanonicalReading,
    device_id: &'a Identity,
    empty_curve: lib_tms::Curve2D,
}

impl<'a> StorageUpdateView<'a> {
    fn new(reading: &'a CanonicalReading, device_id: &'a Identity) -> Self {
        Self {
            reading,
            device_id,
            empty_curve: lib_tms::Curve2D {
                points: heapless::Vec::new(),
            },
        }
    }
}

impl<'a> StorageUpdateSrc for StorageUpdateView<'a> {
    fn device_id(&self) -> &str {
        self.device_id.as_str()
    }
    fn internal_voltage(&self) -> FloatVal {
        FloatVal::Fixed(lib_tms::fields::FixedNum {
            scaled: self.reading.total_voltage_cv as i64,
            frac_digits: 2,
            invert_sign: false,
        })
    }
    fn state_of_charge(&self) -> Option<FloatVal> {
        Some(FloatVal::Fixed(lib_tms::fields::FixedNum {
            scaled: self.reading.soc_dpct as i64,
            frac_digits: 3,
            invert_sign: false,
        }))
    }
    fn available_energy(&self) -> FloatVal {
        // Ah = remaining_capacity_cah / 100; V = total_voltage_cv / 100; J = Ah × V × 3600.
        let available_energy = (self.reading.remaining_capacity_cah as f32 / 100.0)
            * (self.reading.total_voltage_cv as f32 / 100.0)
            * 3600.0;
        FloatVal::F32(available_energy)
    }
    fn hold_time(&self) -> FloatVal {
        FloatVal::Null
    }
    type ChargeTime<'b>
        = &'b lib_tms::Curve2D
    where
        Self: 'b;
    fn charge_time(&self) -> Self::ChargeTime<'_> {
        &self.empty_curve
    }
    type DischargeTime<'b>
        = &'b lib_tms::Curve2D
    where
        Self: 'b;
    fn discharge_time(&self) -> Self::DischargeTime<'_> {
        &self.empty_curve
    }
    type MaxChargeTime<'b>
        = &'b lib_tms::Curve2D
    where
        Self: 'b;
    fn max_charge_time(&self) -> Self::MaxChargeTime<'_> {
        &self.empty_curve
    }
    type MaxDischargeTime<'b>
        = &'b lib_tms::Curve2D
    where
        Self: 'b;
    fn max_discharge_time(&self) -> Self::MaxDischargeTime<'_> {
        &self.empty_curve
    }
    fn max_charge_rate(&self) -> Option<FloatVal> {
        None
    }
    fn max_discharge_rate(&self) -> Option<FloatVal> {
        None
    }
}

impl WireFlat for StorageUpdateWire<'_> {
    fn serialize_flat(&self, sink: &mut impl JsonSink) -> Result<(), WireError> {
        let view = StorageUpdateView::new(self.reading, self.device_id);
        emit_object::<_, lib_tms::generated_fields::storage_update_fields::Chain, _>(&view, sink);
        Ok(())
    }
}
