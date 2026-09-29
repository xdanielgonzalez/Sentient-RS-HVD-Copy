/// Fault/warning flags synthesized into a `0x44` response, one field per bit in the
/// `lib_parse::seplos::fault` `(byte, bit)` map (bytes 3..=10 of the fault INFO payload).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FaultFlags {
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
