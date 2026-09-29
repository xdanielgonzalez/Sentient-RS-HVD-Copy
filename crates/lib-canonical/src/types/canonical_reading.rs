use heapless::Vec;
use serde::{Deserialize, Serialize};

use crate::{MAX_CELLS, MAX_TEMPS};

/// A vendor-neutral battery telemetry reading in raw fixed-point units.
///
/// Scale factors (applied downstream by the mapper): cell voltage = mV; temperature per vendor
/// (Seplos = Kelvin×10, convert `(raw−2731)/10` °C); current/voltage/capacity ×0.01; soc/soh ×0.1.
///
/// # Example
///
/// The model is `postcard`-serializable for internal transport/logging — a bounded, alloc-free
/// round-trip through a caller-owned buffer:
///
/// ```
/// use lib_canonical::CanonicalReading;
///
/// let reading = CanonicalReading {
///     device_address: 1,
///     cell_voltages_mv: [3894, 3901].into_iter().collect(),
/// #     temperatures_raw: [3031, 3028].into_iter().collect(),
/// #     current_ca: -144,
/// #     total_voltage_cv: 5451,
/// #     remaining_capacity_cah: 13_953,
/// #     typical_capacity_cah: 18_000,
/// #     rated_capacity_cah: 18_000,
/// #     soc_dpct: 775,
/// #     soh_dpct: 1000,
/// #     cycles: 2,
/// #     port_voltage_cv: 5453,
///     // … remaining raw-unit fields elided …
/// };
///
/// let mut buf = [0u8; 256];
/// let wire = postcard::to_slice(&reading, &mut buf)?;
/// let decoded: CanonicalReading = postcard::from_bytes(wire)?;
/// assert_eq!(decoded, reading);
/// # Ok::<(), postcard::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalReading {
    /// Source device address (e.g. Seplos DIP address).
    pub device_address: u8,
    /// Per-cell voltages, millivolts (raw).
    pub cell_voltages_mv: Vec<u16, MAX_CELLS>,
    /// Temperature sensor readings, raw vendor units (Seplos: Kelvin×10).
    pub temperatures_raw: Vec<u16, MAX_TEMPS>,
    /// Pack current, signed centi-amps (×0.01 A). Positive = charging (vendor convention;
    /// the TMS mapper inverts sign to the microgrid convention).
    pub current_ca: i16,
    /// Total pack voltage, centi-volts (×0.01 V).
    pub total_voltage_cv: u16,
    /// Remaining capacity, centi-amp-hours (×0.01 Ah).
    pub remaining_capacity_cah: u16,
    /// Typical (nominal) capacity, centi-amp-hours.
    pub typical_capacity_cah: u16,
    /// Rated capacity, centi-amp-hours.
    pub rated_capacity_cah: u16,
    /// State of charge, deci-percent (×0.1 %).
    pub soc_dpct: u16,
    /// State of health, deci-percent (×0.1 %).
    pub soh_dpct: u16,
    /// Charge/discharge cycle count (raw).
    pub cycles: u16,
    /// Port voltage, centi-volts.
    pub port_voltage_cv: u16,
}
