//! Field selection + compact-JSON formatting for [`crate::mesh::condense`]'s priority list. The
//! generic greedy "does it still fit" packing loop lives on the free fn itself (`mod.rs`, since
//! `condense` has no associated type to split an `impl` block from); this file holds the domain
//! judgment call — WHICH fields, in WHAT priority order, formatted HOW.
//!
//! **no_std, no alloc**: every field is formatted directly into the caller's buffer via
//! [`super::types::BoundedWriter`] (a `core::fmt::Write` cursor) — no intermediate `String`/`Vec`.
//!
//! Field set + priority order is a best-effort derivation from `docs/gap-analysis-meteor-parity.md`'s
//! "Meshtastic mesh-radio forwarding" finding's own suggested priority (SOC, pack V, current,
//! min/max cell, temps, faults, location). Values are the *source* fixed-point integers
//! (deci-percent, centi-volt, centi-amp, raw millivolt / vendor temperature units — see
//! `CanonicalReading`'s own field docs for the scale factors), not floating-point engineering
//! units — this keeps the JSON compact and avoids a second, possibly-lossy scale/round step on an
//! already bandwidth-starved link. A receiving app must know these scale factors (and the fault
//! bitmask's bit order, see [`fault_bitmask`]) to render them.
//!
//! **#387 — fault forwarding.** `condense` previously had no fault input at all (a regression vs.
//! the baseline, which does forward faults over mesh — `docs/gap-analysis-meteor-parity.md` lines
//! 29/217). [`crate::mesh::CondenseInput::fault`] closes that gap; the fault field is placed ABOVE
//! location in priority (a fault is the highest-value thing that can ride a degraded link) but
//! below the reading's own core fields, which is the one placement constraint the task brief
//! specifies.
//!
//! **Greedy-loop divergence fix.** The previous version stopped PERMANENTLY at the first field
//! that didn't fit the remaining budget, even though a later, smaller field might still have
//! fit. Each field here is now an independent [`try_field`] attempt (checkpoint + rollback on
//! overflow) rather than a `break` out of a shared loop, so a large field that doesn't fit no
//! longer blocks a smaller, lower-priority one that comes after it.

use super::prelude::*;
use core::fmt::Write as _;

/// The smallest valid JSON object, `{}` — nothing shorter than this can ever hold a field, so
/// [`crate::mesh::condense`] bails out immediately when `max` is under this.
pub(crate) const EMPTY_OBJECT_LEN: usize = 2;

/// Attempt to append one field to `w` via `f`, prefixed by a separating comma unless `*first`.
///
/// On any overflow (from either the separator or the field body), `w` is rolled back to its
/// pre-attempt length — the field is simply skipped, not a hard stop for the fields that follow.
/// Returns whether the field was included, so the caller can leave `*first` untouched on failure.
fn try_field(
    w: &mut BoundedWriter<'_>,
    first: &mut bool,
    f: impl FnOnce(&mut BoundedWriter<'_>) -> core::fmt::Result,
) -> bool {
    let checkpoint = w.len;
    let result: core::fmt::Result = (|| {
        if !*first {
            w.write_char(',')?;
        }
        f(w)
    })();
    match result {
        Ok(()) => {
            *first = false;
            true
        }
        Err(_) => {
            w.len = checkpoint;
            false
        }
    }
}

/// Encode a [`CanonicalFault`]'s sixteen protection/warning flags as a compact bitmask (bit `i` =
/// this list's `i`-th flag, LSB first) instead of sixteen JSON keys:
///
/// 0 `high_voltage_protection` · 1 `low_voltage_protection` · 2
/// `high_temperature_charge_protection` · 3 `low_temperature_charge_protection` · 4
/// `charge_overcurrent_protection` · 5 `discharge_overcurrent_protection` · 6
/// `short_circuit_protection` · 7 `mosfet_fault` · 8 `cell_balance_warning` · 9
/// `low_capacity_warning` · 10 `high_temperature_discharge_warning` · 11
/// `low_temperature_discharge_warning` · 12 `low_voltage_warning` · 13 `high_voltage_warning` · 14
/// `discharge_current_warning` · 15 `charge_current_warning`.
///
/// This bit order is defined ONLY here — a receiving app must know it to decode the `"f"` field.
pub(crate) fn fault_bitmask(fault: &CanonicalFault) -> u16 {
    let flags = [
        fault.high_voltage_protection,
        fault.low_voltage_protection,
        fault.high_temperature_charge_protection,
        fault.low_temperature_charge_protection,
        fault.charge_overcurrent_protection,
        fault.discharge_overcurrent_protection,
        fault.short_circuit_protection,
        fault.mosfet_fault,
        fault.cell_balance_warning,
        fault.low_capacity_warning,
        fault.high_temperature_discharge_warning,
        fault.low_temperature_discharge_warning,
        fault.low_voltage_warning,
        fault.high_voltage_warning,
        fault.discharge_current_warning,
        fault.charge_current_warning,
    ];
    let mut mask: u16 = 0;
    for (i, &flag) in flags.iter().enumerate() {
        if flag {
            mask |= 1 << i;
        }
    }
    mask
}

/// Greedily write `input`'s condensed fields into `w`, in priority order: SOC, pack voltage,
/// current, min/max cell, temperatures, fault bitmask, then (only when this crate's `locate`
/// feature is enabled) location. Each field is an independent [`try_field`] attempt — a field
/// that doesn't fit the remaining budget is simply skipped, never a hard stop for the fields that
/// follow (see this module's doc comment).
pub(crate) fn write_fields(input: &CondenseInput<'_>, w: &mut BoundedWriter<'_>) {
    let reading = input.reading;
    let mut first = true;

    // 1. State of charge (deci-percent, ×0.1 %) — the single number an operator most wants off a
    //    stranded/limited-bandwidth mesh link.
    try_field(w, &mut first, |w| write!(w, "\"soc\":{}", reading.soc_dpct));
    // 2. Pack voltage (centi-volts, ×0.01 V).
    try_field(w, &mut first, |w| {
        write!(w, "\"v\":{}", reading.total_voltage_cv)
    });
    // 3. Pack current (signed centi-amps, ×0.01 A).
    try_field(w, &mut first, |w| write!(w, "\"i\":{}", reading.current_ca));
    // 4. Min/max cell voltage (millivolts) — the two extremes flag imbalance without shipping
    //    every individual cell.
    if let Some(min) = reading.cell_voltages_mv.iter().copied().min() {
        try_field(w, &mut first, |w| write!(w, "\"cmin\":{min}"));
    }
    if let Some(max) = reading.cell_voltages_mv.iter().copied().max() {
        try_field(w, &mut first, |w| write!(w, "\"cmax\":{max}"));
    }
    // 5. Temperatures (raw vendor units) — the full sensor array; each entry is small enough that
    //    this is usually still affordable even on a tight budget.
    if !reading.temperatures_raw.is_empty() {
        try_field(w, &mut first, |w| {
            w.write_str("\"t\":[")?;
            for (i, temp) in reading.temperatures_raw.iter().enumerate() {
                if i > 0 {
                    w.write_char(',')?;
                }
                write!(w, "{temp}")?;
            }
            w.write_char(']')
        });
    }
    // 6. Fault/warning bitmask (#387) — the highest-value thing that can ride a degraded link,
    //    prioritized above location but below the reading's own core fields.
    if let Some(fault) = input.fault {
        let mask = fault_bitmask(fault);
        try_field(w, &mut first, |w| write!(w, "\"f\":{mask}"));
    }
    // 7. Location — lowest priority; only present when a fix was threaded through (and only
    //    compiled at all when `locate` is enabled — the MCU's bare `mesh` build never has one).
    // Rounded to 5 decimal places (~1 m precision) to bound size rather than emitting a
    // full-precision f64.
    #[cfg(feature = "locate")]
    if let Some(fix) = input.loc {
        try_field(w, &mut first, |w| write!(w, "\"lat\":{:.5}", fix.lat));
        try_field(w, &mut first, |w| write!(w, "\"lon\":{:.5}", fix.lon));
    }
}

#[cfg(test)]
// Test code may unwrap/expect/panic; the workspace lint gate denies these in production logic
// only.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use heapless::Vec as HVec;

    fn reading(soc_dpct: u16, cells: &[u16], temps: &[u16]) -> CanonicalReading {
        let mut cell_voltages_mv: HVec<u16, { lib_parse::canonical::MAX_CELLS }> = HVec::new();
        for &c in cells {
            let _ = cell_voltages_mv.push(c);
        }
        let mut temperatures_raw: HVec<u16, { lib_parse::canonical::MAX_TEMPS }> = HVec::new();
        for &t in temps {
            let _ = temperatures_raw.push(t);
        }
        CanonicalReading {
            device_address: 0,
            cell_voltages_mv,
            temperatures_raw,
            current_ca: -144,
            total_voltage_cv: 5451,
            remaining_capacity_cah: 13953,
            typical_capacity_cah: 18000,
            rated_capacity_cah: 18000,
            soc_dpct,
            soh_dpct: 1000,
            cycles: 2,
            port_voltage_cv: 5453,
        }
    }

    fn tripped_fault() -> CanonicalFault {
        CanonicalFault {
            charge_overcurrent_protection: true,
            ..CanonicalFault::default()
        }
    }

    /// Builds a [`CondenseInput`] with no location tag (works identically whether or not this
    /// crate's `locate` feature is enabled — the field is simply absent from the struct when it
    /// isn't).
    fn input<'a>(
        reading: &'a CanonicalReading,
        fault: Option<&'a CanonicalFault>,
    ) -> CondenseInput<'a> {
        CondenseInput {
            reading,
            fault,
            #[cfg(feature = "locate")]
            loc: None,
        }
    }

    fn condense_str<'o>(inp: &CondenseInput<'_>, max: usize, out: &'o mut [u8]) -> &'o str {
        let n = crate::mesh::condense(inp, max, out);
        core::str::from_utf8(&out[..n]).expect("valid utf8")
    }

    // ---- fault_bitmask ------------------------------------------------------------------------

    #[test]
    fn fault_bitmask_is_zero_when_nothing_is_tripped() {
        assert_eq!(fault_bitmask(&CanonicalFault::default()), 0);
    }

    #[test]
    fn fault_bitmask_sets_the_bit_for_each_flag() {
        assert_eq!(fault_bitmask(&tripped_fault()), 1 << 4);
        let hv = CanonicalFault {
            high_voltage_protection: true,
            ..CanonicalFault::default()
        };
        assert_eq!(fault_bitmask(&hv), 1);
        let last = CanonicalFault {
            charge_current_warning: true,
            ..CanonicalFault::default()
        };
        assert_eq!(fault_bitmask(&last), 1 << 15);
    }

    // ---- crate::mesh::condense (the greedy budget loop) ----------------------------------------

    #[test]
    fn everything_fits_under_a_generous_budget() {
        let r = reading(775, &[3800, 3820], &[2981, 2990]);
        let fault = tripped_fault();
        let mut out = [0u8; 256];
        let s = condense_str(&input(&r, Some(&fault)), 200, &mut out);
        assert!(s.starts_with('{') && s.ends_with('}'));
        for key in ["soc", "v", "i", "cmin", "cmax", "t", "f"] {
            assert!(s.contains(key), "expected {key} in {s}");
        }
        assert!(s.len() <= 200);
    }

    #[test]
    fn a_tiny_budget_keeps_only_the_highest_priority_field() {
        let r = reading(775, &[3800, 3820], &[2981, 2990]);
        let mut out = [0u8; 32];
        let s = condense_str(&input(&r, None), 12, &mut out);
        assert_eq!(s, "{\"soc\":775}");
        assert!(s.len() <= 12);
    }

    #[test]
    fn budget_too_small_for_any_field_yields_the_empty_object() {
        let r = reading(775, &[3800], &[]);
        let mut out = [0u8; 8];
        let s = condense_str(&input(&r, None), 3, &mut out);
        assert_eq!(s, "{}");
    }

    #[test]
    fn budget_below_two_bytes_writes_nothing() {
        let r = reading(775, &[], &[]);
        let mut out = [0xAAu8; 8]; // pre-existing content must be ignored, not appended to
        let n = crate::mesh::condense(&input(&r, None), 1, &mut out);
        assert_eq!(n, 0);
    }

    #[test]
    fn stopping_at_the_first_field_that_does_not_fit_never_exceeds_max() {
        // A budget that fits soc+v+i but not cmin too — later, larger fields must not push the
        // output past `max` even though the greedy loop now keeps trying smaller ones after them.
        let r = reading(775, &[3800, 3820], &[2981]);
        let max = "{\"soc\":775,\"v\":5451,\"i\":-144}".len();
        let mut out = [0u8; 64];
        let s = condense_str(&input(&r, None), max, &mut out);
        assert!(
            s.len() <= max,
            "condense must never exceed the requested budget"
        );
        assert!(s.contains("soc") && s.contains("\"v\"") && s.contains("\"i\""));
        assert!(!s.contains("cmin"), "cmin must not fit in this budget: {s}");
    }

    /// The greedy-loop divergence fix (step 5): an oversized field (many temperature sensors)
    /// that doesn't fit must NOT block a smaller, lower-priority field (the fault bitmask) that
    /// comes after it and DOES fit.
    #[test]
    fn a_large_field_that_does_not_fit_no_longer_blocks_smaller_ones_after_it() {
        let r = reading(775, &[], &[100, 200, 300, 400, 500, 600, 700, 800]);
        let fault = tripped_fault();
        let max = "{\"soc\":775,\"f\":16}".len();
        let mut out = [0u8; 64];
        let s = condense_str(&input(&r, Some(&fault)), max, &mut out);
        assert_eq!(s, "{\"soc\":775,\"f\":16}");
        assert!(
            !s.contains("\"t\":"),
            "the oversized temps field correctly does not fit: {s}"
        );
    }

    /// A second call with a SMALLER result must not leave any trailing bytes from a previous,
    /// larger call readable within the new `out[..n]` — every byte in that range is written fresh
    /// by the current call (no `Vec`-style `clear`-then-append needed with a buffer-based API,
    /// but the invariant it protected still matters).
    #[test]
    fn condense_does_not_leak_a_previous_larger_calls_content() {
        let r = reading(775, &[3800, 3820], &[2981, 2990]);
        let fault = tripped_fault();
        let mut out = [0u8; 64];
        let n1 = crate::mesh::condense(&input(&r, Some(&fault)), 64, &mut out);
        assert!(n1 > 11);
        let n2 = crate::mesh::condense(&input(&r, None), 12, &mut out);
        let s2 = core::str::from_utf8(&out[..n2]).expect("valid utf8");
        assert_eq!(s2, "{\"soc\":775}");
    }

    #[cfg(feature = "locate")]
    mod with_location {
        use super::*;
        use crate::locate::{LocationFix, LocationSource};

        fn fix() -> LocationFix {
            LocationFix {
                lat: 37.774_929,
                lon: -122.419_416,
                alt_m: 12.0,
                source: LocationSource::GpsNmea,
                ts_ms: 1000,
            }
        }

        #[test]
        fn priority_order_is_soc_v_i_cmin_cmax_t_fault_then_location() {
            let r = reading(775, &[3800, 3820, 3810], &[2981, 2990]);
            let fault = tripped_fault();
            let fx = fix();
            let input = CondenseInput {
                reading: &r,
                fault: Some(&fault),
                loc: Some(&fx),
            };
            let mut out = [0u8; 256];
            let s = condense_str(&input, 256, &mut out);
            let pos = |needle: &str| {
                s.find(needle)
                    .unwrap_or_else(|| panic!("{needle} missing from {s}"))
            };
            let order = [
                pos("\"soc\""),
                pos("\"v\""),
                pos("\"i\""),
                pos("\"cmin\""),
                pos("\"cmax\""),
                pos("\"t\""),
                pos("\"f\""),
                pos("\"lat\""),
                pos("\"lon\""),
            ];
            assert!(
                order.windows(2).all(|w| w[0] < w[1]),
                "fields out of priority order: {s}"
            );
        }

        #[test]
        fn location_absent_when_no_fix_is_threaded_through() {
            let r = reading(775, &[], &[]);
            let input = CondenseInput {
                reading: &r,
                fault: None,
                loc: None,
            };
            let mut out = [0u8; 64];
            let s = condense_str(&input, 200, &mut out);
            assert!(!s.contains("lat"));
        }
    }
}
