use heapless::{String, Vec};

use super::super::{MAX_CELLS, MAX_SERIAL, MAX_TEMPS};
use super::fault_flags::FaultFlags;

/// The simulated battery's state — the source of truth [`super::seplos_sim::SeplosSim`] encodes into `0x42`/`0x44`/
/// `0xA4` responses. Field shapes/units mirror [`lib_parse::canonical::CanonicalReading`] and
/// [`lib_parse::canonical::CanonicalFault`] exactly, since this struct's job is to encode the
/// byte layout their decoders expect.
#[derive(Debug, Clone, PartialEq)]
pub struct BatteryState {
    pub cell_voltages_mv: Vec<u16, MAX_CELLS>,
    pub temperatures_raw: Vec<u16, MAX_TEMPS>,
    pub current_ca: i16,
    pub total_voltage_cv: u16,
    pub remaining_capacity_cah: u16,
    pub typical_capacity_cah: u16,
    pub soc_dpct: u16,
    pub rated_capacity_cah: u16,
    pub cycles: u16,
    pub soh_dpct: u16,
    pub port_voltage_cv: u16,
    pub faults: FaultFlags,
    pub serial: String<MAX_SERIAL>,
}
