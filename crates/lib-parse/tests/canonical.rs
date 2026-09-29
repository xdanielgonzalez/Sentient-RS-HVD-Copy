//! `CanonicalFault::any()` — the vendor-neutral aggregate protection/warning check.
//!
//! `any()` is the signal the downstream TMS mapper (and the poll loop) uses to decide whether a
//! device needs an `ActiveDiagnosticState` publish; it must be true iff at least one of the 16
//! flags is set, and false when every flag is clear (the all-clear case, which is the common
//! steady-state reading and must not false-positive).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_parse::canonical::CanonicalFault;

#[test]
fn default_is_all_clear() {
    let fault = CanonicalFault::default();
    assert!(
        !fault.any(),
        "a freshly defaulted fault must report no active flags"
    );
}

#[test]
fn any_flag_set_makes_any_true() {
    // Exercise a representative flag from each half of the struct (protection vs. warning) so a
    // future field reorder can't silently drop one out of the `any()` OR-chain unnoticed.
    let fault = CanonicalFault {
        high_voltage_protection: true,
        ..CanonicalFault::default()
    };
    assert!(fault.any(), "a set protection flag must flip any() true");

    let fault = CanonicalFault {
        charge_current_warning: true,
        ..CanonicalFault::default()
    };
    assert!(fault.any(), "a set warning flag must flip any() true");
}

#[test]
fn every_flag_independently_trips_any() {
    // Loop the OR-chain through every flag position via the mutable field accessors, proving none
    // was dropped from `any()` — a single "set the last flag" test would hide an early return bug.
    let flags: [fn(&mut CanonicalFault); 16] = [
        |f| f.high_voltage_protection = true,
        |f| f.low_voltage_protection = true,
        |f| f.high_temperature_charge_protection = true,
        |f| f.low_temperature_charge_protection = true,
        |f| f.charge_overcurrent_protection = true,
        |f| f.discharge_overcurrent_protection = true,
        |f| f.short_circuit_protection = true,
        |f| f.mosfet_fault = true,
        |f| f.cell_balance_warning = true,
        |f| f.low_capacity_warning = true,
        |f| f.high_temperature_discharge_warning = true,
        |f| f.low_temperature_discharge_warning = true,
        |f| f.low_voltage_warning = true,
        |f| f.high_voltage_warning = true,
        |f| f.discharge_current_warning = true,
        |f| f.charge_current_warning = true,
    ];
    for set_flag in flags {
        let mut fault = CanonicalFault::default();
        set_flag(&mut fault);
        assert!(fault.any(), "every individual flag must trip any()");
    }
}

#[test]
fn device_address_is_carried_independently_of_flags() {
    // device_address plays no part in any(); a nonzero address with no flags must still read clear.
    let fault = CanonicalFault {
        device_address: 7,
        ..CanonicalFault::default()
    };
    assert_eq!(fault.device_address, 7);
    assert!(!fault.any());
}

#[test]
fn fault_equality_and_debug_are_field_sensitive() {
    // Sanity on the derived Eq/Debug (used by other crates' assert_eq! on faults) — two faults that
    // differ in exactly one flag must compare unequal and each must render distinctly.
    let clear = CanonicalFault::default();
    let tripped = CanonicalFault {
        mosfet_fault: true,
        ..CanonicalFault::default()
    };

    assert_ne!(clear, tripped);
    assert_ne!(format!("{clear:?}"), format!("{tripped:?}"));
}
