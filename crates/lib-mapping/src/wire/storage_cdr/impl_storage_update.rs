//! XCDR2 CDR binding for `StorageUpdate` — CDR twin of `wire::storage::impl_storage_update`.
//! Values UNSCALED (real `f32`); the four `*_time` curves are empty (`Curve2D` with no points).

use lib_parse::canonical::CanonicalReading;
use lib_tms::fields::Absent;
use lib_tms::generated_cdr::curve2_d_cdr::Curve2DCdrSrc;
use lib_tms::generated_cdr::storage_update_cdr::StorageUpdateCdrSrc;
use lib_tms::Identity;

/// An empty `Curve2D` CDR view (no points) — the four `StorageUpdate` time-curve fields have no
/// canonical source (empty on the JSON path too).
pub struct EmptyCurveCdrView;

impl Curve2DCdrSrc for EmptyCurveCdrView {
    type PointsItem<'a>
        = Absent
    where
        Self: 'a;
    fn points(&self) -> impl Iterator<Item = Self::PointsItem<'_>> + '_ {
        core::iter::empty()
    }
}

/// `StorageUpdate` CDR view — battery energy storage state: voltage, SOC, available energy, empty
/// time curves, absent rates.
pub struct StorageUpdateCdrView<'a> {
    pub reading: &'a CanonicalReading,
    pub device_id: &'a Identity,
}

impl<'a> StorageUpdateCdrSrc for StorageUpdateCdrView<'a> {
    fn device_id(&self) -> &str {
        self.device_id.as_str()
    }
    fn internal_voltage(&self) -> f32 {
        self.reading.total_voltage_cv as f32 / 100.0
    }
    fn state_of_charge(&self) -> Option<f32> {
        Some(self.reading.soc_dpct as f32 / 1000.0)
    }
    fn available_energy(&self) -> f32 {
        // Ah = remaining_capacity_cah / 100; V = total_voltage_cv / 100; J = Ah × V × 3600.
        (self.reading.remaining_capacity_cah as f32 / 100.0)
            * (self.reading.total_voltage_cv as f32 / 100.0)
            * 3600.0
    }
    fn hold_time(&self) -> f32 {
        f32::NAN
    }
    type ChargeTime<'b>
        = EmptyCurveCdrView
    where
        Self: 'b;
    fn charge_time(&self) -> Self::ChargeTime<'_> {
        EmptyCurveCdrView
    }
    type DischargeTime<'b>
        = EmptyCurveCdrView
    where
        Self: 'b;
    fn discharge_time(&self) -> Self::DischargeTime<'_> {
        EmptyCurveCdrView
    }
    type MaxChargeTime<'b>
        = EmptyCurveCdrView
    where
        Self: 'b;
    fn max_charge_time(&self) -> Self::MaxChargeTime<'_> {
        EmptyCurveCdrView
    }
    type MaxDischargeTime<'b>
        = EmptyCurveCdrView
    where
        Self: 'b;
    fn max_discharge_time(&self) -> Self::MaxDischargeTime<'_> {
        EmptyCurveCdrView
    }
    fn max_charge_rate(&self) -> Option<f32> {
        None
    }
    fn max_discharge_rate(&self) -> Option<f32> {
        None
    }
}
