use serde::{Deserialize, Serialize};

use super::inverter_mode::InverterMode;
use super::inverter_ratings::InverterRatings;

/// A vendor-neutral inverter telemetry reading in raw fixed-point device units. Novel canonical
/// type (no MIL-STD-3071 analogue) produced by the RS-232 inverter leg; PROVISIONAL. Scaling to
/// engineering units is the mapper's concern (suffix names the raw unit: `dv`=deci-volt,
/// `dhz`=deci-hertz, `cv`=centi-volt, `da`=deci-amp, `c`=degree Celsius).
///
/// # Example
///
/// The `status_bits` accessors decode the device status word — here an off-grid pack running
/// the load from battery while PV charges it:
///
/// ```
/// use lib_canonical::{CanonicalInverterReading, InverterMode};
///
/// let reading = CanonicalInverterReading {
///     status_bits: 0b1_0110, // bit4 load on, bit2 charging, bit1 solar charging
///     mode: InverterMode::Battery,
///     ratings: None,
/// #     grid_voltage_dv: 0,
/// #     grid_frequency_dhz: 0,
/// #     ac_output_voltage_dv: 2301,
/// #     ac_output_frequency_dhz: 499,
/// #     ac_output_apparent_power_va: 350,
/// #     ac_output_active_power_w: 320,
/// #     bus_voltage_v: 400,
/// #     battery_voltage_cv: 2712,
/// #     battery_charging_current_a: 10,
/// #     battery_discharge_current_a: 0,
/// #     inverter_temperature_c: 38,
/// #     pv_current_da: 52,
/// #     pv_voltage_dv: 3450,
/// #     pv_charging_power_w: 180,
///     // … raw-unit telemetry fields elided …
/// };
///
/// assert!(reading.load_on());
/// assert!(reading.charging());
/// assert!(reading.solar_charging());
/// assert!(!reading.ac_charging()); // bit0 clear — not charging from the grid
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalInverterReading {
    pub grid_voltage_dv: u16,
    pub grid_frequency_dhz: u16,
    pub ac_output_voltage_dv: u16,
    pub ac_output_frequency_dhz: u16,
    pub ac_output_apparent_power_va: u16,
    pub ac_output_active_power_w: u16,
    pub bus_voltage_v: u16,
    pub battery_voltage_cv: u16,
    pub battery_charging_current_a: u16,
    pub battery_discharge_current_a: u16,
    pub inverter_temperature_c: i16,
    pub pv_current_da: u16,
    pub pv_voltage_dv: u16,
    pub pv_charging_power_w: u16,
    /// Device status flag bits (bit4=load on, bit2=charging, bit1=solar charging, bit0=AC charging).
    pub status_bits: u8,
    pub mode: InverterMode,
    pub ratings: Option<InverterRatings>,
}
