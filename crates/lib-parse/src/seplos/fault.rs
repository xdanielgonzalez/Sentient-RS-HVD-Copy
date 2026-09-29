//! Seplos `0x44` fault/warning decode → [`CanonicalFault`].
//!
//! Decoded as absolute `(byte, bit)` tests over the INFO payload. This bit map is **empirical**
//! (observed on live hardware in the prototype), NOT confirmed against the Seplos spec — flagged
//! in the TMS compliance report and to be verified against real captures. A missing INFO byte
//! reads as all-clear for its flags (never an error).
//!
//! Bit map (byte index, bit index → flag):
//! (3,0) high_voltage_protection · (3,1) low_voltage_protection · (4,0) high_temperature_charge_protection ·
//! (4,1) low_temperature_charge_protection · (5,0) charge_overcurrent_protection ·
//! (5,1) discharge_overcurrent_protection · (5,2) short_circuit_protection · (6,0) mosfet_fault ·
//! (7,0) cell_balance_warning · (7,1) low_capacity_warning · (8,0) high_temperature_discharge_warning ·
//! (8,1) low_temperature_discharge_warning · (9,0) low_voltage_warning · (9,1) high_voltage_warning ·
//! (10,0) discharge_current_warning · (10,1) charge_current_warning.

use super::error::SeplosError;
use crate::canonical::CanonicalFault;

/// Decode a verified Seplos `0x44` fault frame into the vendor-neutral canonical fault flags.
///
/// FILL: implement per the module-doc `(byte, bit)` map. Set `device_address` from `frame.adr`.
/// For each mapped flag, read `frame.info.get(byte)`; if the byte is present, test its bit;
/// a missing byte leaves that flag `false` (never an error). No `unwrap`/`expect`, no panic,
/// no indexing that can panic.
pub fn decode_fault(frame: &super::frame::ResponseFrame) -> Result<CanonicalFault, SeplosError> {
    let bit = |byte: usize, b: u8| frame.info.get(byte).is_some_and(|v| (v >> b) & 1 == 1);

    Ok(CanonicalFault {
        device_address: frame.adr,
        high_voltage_protection: bit(3, 0),
        low_voltage_protection: bit(3, 1),
        high_temperature_charge_protection: bit(4, 0),
        low_temperature_charge_protection: bit(4, 1),
        charge_overcurrent_protection: bit(5, 0),
        discharge_overcurrent_protection: bit(5, 1),
        short_circuit_protection: bit(5, 2),
        mosfet_fault: bit(6, 0),
        cell_balance_warning: bit(7, 0),
        low_capacity_warning: bit(7, 1),
        high_temperature_discharge_warning: bit(8, 0),
        low_temperature_discharge_warning: bit(8, 1),
        low_voltage_warning: bit(9, 0),
        high_voltage_warning: bit(9, 1),
        discharge_current_warning: bit(10, 0),
        charge_current_warning: bit(10, 1),
    })
}
