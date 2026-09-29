//! `postcard` round-trip for the vendor-neutral canonical battery model (`CanonicalReading` /
//! `CanonicalFault`), owned by this crate (sibling of the TMS model in `lib-tms`) so the decoded
//! Seplos model is itself postcard-serializable for internal transport/logging.
//!
//! Every field on both types is an integer or bool (no floats), so a direct `assert_eq!` on the
//! decoded struct is sufficient — unlike the TMS types' `f32` fields, there is no `NaN != NaN`
//! subtlety to route around here.

// Test code may unwrap/expect; the workspace denies these in logic only.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use heapless::Vec as HVec;
use lib_canonical::{CanonicalFault, CanonicalReading};

fn sample_reading() -> CanonicalReading {
    let mut cell_voltages_mv: HVec<u16, { lib_canonical::MAX_CELLS }> = HVec::new();
    cell_voltages_mv.push(3894).unwrap();
    cell_voltages_mv.push(3901).unwrap();

    let mut temperatures_raw: HVec<u16, { lib_canonical::MAX_TEMPS }> = HVec::new();
    temperatures_raw.push(3031).unwrap();
    temperatures_raw.push(3028).unwrap();

    CanonicalReading {
        device_address: 1,
        cell_voltages_mv,
        temperatures_raw,
        current_ca: -144,
        total_voltage_cv: 5451,
        remaining_capacity_cah: 13953,
        typical_capacity_cah: 18000,
        rated_capacity_cah: 18000,
        soc_dpct: 775,
        soh_dpct: 1000,
        cycles: 2,
        port_voltage_cv: 5453,
    }
}

fn sample_fault() -> CanonicalFault {
    CanonicalFault {
        device_address: 1,
        high_voltage_protection: true,
        low_voltage_protection: false,
        high_temperature_charge_protection: true,
        low_temperature_charge_protection: false,
        charge_overcurrent_protection: false,
        discharge_overcurrent_protection: true,
        short_circuit_protection: false,
        mosfet_fault: false,
        cell_balance_warning: true,
        low_capacity_warning: false,
        high_temperature_discharge_warning: false,
        low_temperature_discharge_warning: true,
        low_voltage_warning: false,
        high_voltage_warning: false,
        discharge_current_warning: false,
        charge_current_warning: true,
    }
}

#[test]
fn canonical_reading_round_trips_through_postcard() {
    let original = sample_reading();

    let mut buf = [0u8; 256];
    let used = postcard::to_slice(&original, &mut buf).expect("encode");
    let (decoded, remainder): (CanonicalReading, &[u8]) =
        postcard::take_from_bytes(used).expect("decode");

    assert_eq!(decoded, original);
    assert!(
        remainder.is_empty(),
        "decode must consume the whole payload"
    );
}

#[test]
fn canonical_reading_empty_sequences_round_trip() {
    // Zero cells/temps is a legal reading (e.g. a partial/errored telemetry decode never reaches
    // the mapper, but bounds-wise this is the minimum shape) — exercise the empty-vec edge.
    let original = CanonicalReading {
        device_address: 0,
        cell_voltages_mv: HVec::new(),
        temperatures_raw: HVec::new(),
        current_ca: 0,
        total_voltage_cv: 0,
        remaining_capacity_cah: 0,
        typical_capacity_cah: 0,
        rated_capacity_cah: 0,
        soc_dpct: 0,
        soh_dpct: 0,
        cycles: 0,
        port_voltage_cv: 0,
    };

    let mut buf = [0u8; 64];
    let used = postcard::to_slice(&original, &mut buf).expect("encode");
    let decoded: CanonicalReading = postcard::from_bytes(used).expect("decode");

    assert_eq!(decoded, original);
}

#[test]
fn canonical_fault_round_trips_through_postcard() {
    let original = sample_fault();

    let mut buf = [0u8; 64];
    let used = postcard::to_slice(&original, &mut buf).expect("encode");
    let decoded: CanonicalFault = postcard::from_bytes(used).expect("decode");

    assert_eq!(decoded, original);
    assert!(decoded.any());
}

#[test]
fn canonical_fault_default_all_clear_round_trips_through_postcard() {
    let original = CanonicalFault::default();
    assert!(!original.any());

    let mut buf = [0u8; 64];
    let used = postcard::to_slice(&original, &mut buf).expect("encode");
    let decoded: CanonicalFault = postcard::from_bytes(used).expect("decode");

    assert_eq!(decoded, original);
    assert!(!decoded.any());
}
