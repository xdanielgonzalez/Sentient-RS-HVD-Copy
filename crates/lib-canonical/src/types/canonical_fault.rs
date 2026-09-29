use serde::{Deserialize, Serialize};

/// Vendor-neutral battery protection/warning flags. A vendor parser sets the flags its protocol
/// reports; unsupported flags stay `false`.
///
/// # Example
///
/// ```
/// use lib_canonical::CanonicalFault;
///
/// // A parser sets only the flags its protocol reports; everything else stays `false`.
/// let fault = CanonicalFault {
///     device_address: 1,
///     low_voltage_warning: true,
///     ..Default::default()
/// };
/// assert!(fault.low_voltage_warning);
/// assert!(!fault.high_voltage_protection); // unsupported/unset flags read false
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CanonicalFault {
    /// Source device address.
    pub device_address: u8,
    pub high_voltage_protection: bool,
    pub low_voltage_protection: bool,
    pub high_temperature_charge_protection: bool,
    pub low_temperature_charge_protection: bool,
    pub charge_overcurrent_protection: bool,
    pub discharge_overcurrent_protection: bool,
    pub short_circuit_protection: bool,
    pub mosfet_fault: bool,
    pub cell_balance_warning: bool,
    pub low_capacity_warning: bool,
    pub high_temperature_discharge_warning: bool,
    pub low_temperature_discharge_warning: bool,
    pub low_voltage_warning: bool,
    pub high_voltage_warning: bool,
    pub discharge_current_warning: bool,
    pub charge_current_warning: bool,
}
