//! Shared step-support helpers reused across every feature-area step module (`mapping`,
//! `resilience`, `security`, `mqtt`, `tms`).
//!
//! **Area submodules must reuse these via `super::support::*` (or `crate::steps::support::*`),
//! never re-implement their own copy.** That keeps step grammar identical across areas — e.g.
//! every area that matches a `{word}` fault flag, parses a raw-byte Example, or asserts a typed
//! error variant name does it exactly one way — and gives future crate-shape changes (a new fault
//! flag, a differently-formatted error type) exactly one call site to update.
//!
//! `dev()`/`now()`/`approx()`/`TOL` (the deterministic device id / timestamp / float-tolerance
//! fixtures) already live in `tests/cucumber.rs` as `pub(crate)` and are reachable from every step
//! module via `crate::{dev, now, approx, TOL}` — no duplication needed here.

use lib_parse::canonical::CanonicalFault;

/// Set one of `CanonicalFault`'s 16 boolean flags by its Gherkin `{word}` name. Panics on an
/// unrecognised name (a feature-file typo, not a runtime condition to handle gracefully).
///
/// Shared across every feature area that sets a `CanonicalFault` flag by name (`mapping`,
/// `resilience`) — do not re-implement this match arm list elsewhere.
///
/// `#[allow(dead_code)]`: `mapping.rs` predates this shared helper and still carries its own
/// private copy; unused here until a caller (`resilience`, or a `mapping` cleanup) switches over —
/// this is scaffolding, not dead weight; remove the allow once a caller lands.
#[allow(dead_code)]
pub(crate) fn set_fault_flag(fault: &mut CanonicalFault, name: &str) {
    let slot = match name {
        "high_voltage_protection" => &mut fault.high_voltage_protection,
        "low_voltage_protection" => &mut fault.low_voltage_protection,
        "high_temperature_charge_protection" => &mut fault.high_temperature_charge_protection,
        "low_temperature_charge_protection" => &mut fault.low_temperature_charge_protection,
        "charge_overcurrent_protection" => &mut fault.charge_overcurrent_protection,
        "discharge_overcurrent_protection" => &mut fault.discharge_overcurrent_protection,
        "short_circuit_protection" => &mut fault.short_circuit_protection,
        "mosfet_fault" => &mut fault.mosfet_fault,
        "cell_balance_warning" => &mut fault.cell_balance_warning,
        "low_capacity_warning" => &mut fault.low_capacity_warning,
        "high_temperature_discharge_warning" => &mut fault.high_temperature_discharge_warning,
        "low_temperature_discharge_warning" => &mut fault.low_temperature_discharge_warning,
        "low_voltage_warning" => &mut fault.low_voltage_warning,
        "high_voltage_warning" => &mut fault.high_voltage_warning,
        "discharge_current_warning" => &mut fault.discharge_current_warning,
        "charge_current_warning" => &mut fault.charge_current_warning,
        other => panic!("unknown fault flag {other:?} in feature file"),
    };
    *slot = true;
}

/// Read one of `CanonicalFault`'s 16 boolean flags by its Gherkin `{word}` name. See
/// [`set_fault_flag`] for the shared name list — keep both in sync.
///
/// `#[allow(dead_code)]`: see [`set_fault_flag`] — unused here until a caller switches over.
#[allow(dead_code)]
pub(crate) fn get_fault_flag(fault: &CanonicalFault, name: &str) -> bool {
    match name {
        "high_voltage_protection" => fault.high_voltage_protection,
        "low_voltage_protection" => fault.low_voltage_protection,
        "high_temperature_charge_protection" => fault.high_temperature_charge_protection,
        "low_temperature_charge_protection" => fault.low_temperature_charge_protection,
        "charge_overcurrent_protection" => fault.charge_overcurrent_protection,
        "discharge_overcurrent_protection" => fault.discharge_overcurrent_protection,
        "short_circuit_protection" => fault.short_circuit_protection,
        "mosfet_fault" => fault.mosfet_fault,
        "cell_balance_warning" => fault.cell_balance_warning,
        "low_capacity_warning" => fault.low_capacity_warning,
        "high_temperature_discharge_warning" => fault.high_temperature_discharge_warning,
        "low_temperature_discharge_warning" => fault.low_temperature_discharge_warning,
        "low_voltage_warning" => fault.low_voltage_warning,
        "high_voltage_warning" => fault.high_voltage_warning,
        "discharge_current_warning" => fault.discharge_current_warning,
        "charge_current_warning" => fault.charge_current_warning,
        other => panic!("unknown fault flag {other:?} in feature file"),
    }
}

/// Parse a Gherkin hex-string Example (e.g. `"DEADBEEF"` or the whitespace-separated `"DE AD BE
/// EF"`, case-insensitive either way) into raw bytes. Panics on malformed hex — a feature-file
/// typo, not a runtime condition to handle gracefully.
///
/// Shared by every feature area whose Examples table carries a raw byte-vector (mqtt payloads,
/// tms frames) — do not re-implement a hex parser elsewhere.
pub(crate) fn parse_hex_bytes(s: &str) -> Vec<u8> {
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        cleaned.len().is_multiple_of(2),
        "hex string {s:?} has an odd number of hex digits"
    );
    (0..cleaned.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&cleaned[i..i + 2], 16).unwrap_or_else(|e| {
                panic!("invalid hex byte {:?} in {s:?}: {e}", &cleaned[i..i + 2])
            })
        })
        .collect()
}

/// Extract the bare enum variant name from a `Debug`-formatted value — e.g. `"Expired"` from both
/// unit variant `Expired` and struct/tuple variants like `Expired { at: 42 }` / `NotApproved(Ecdsa)`
/// (everything up to the first non-identifier character).
///
/// Shared by every feature area that asserts "the error is variant X" against a typed Rust error
/// (`resilience`/`security`/`mqtt`) without hardcoding each error type's exact Display wording.
pub(crate) fn error_variant_name(value: &impl core::fmt::Debug) -> String {
    format!("{value:?}")
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}
