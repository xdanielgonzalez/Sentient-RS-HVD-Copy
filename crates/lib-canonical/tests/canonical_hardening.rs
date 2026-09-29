//! Hardening tests for the vendor-neutral canonical battery model (`CanonicalReading` /
//! `CanonicalFault`): full-capacity round-trips at scalar extremes, a decode truncation sweep
//! (fail-closed, never panics), buffer-too-small encode, `take_from_bytes` remainder exactness,
//! and a per-flag `CanonicalFault::any()` sweep that kills dropped-clause mutants in the OR chain.
//!
//! Complements `canonical_postcard_roundtrip.rs` (basic shapes) rather than duplicating it.

// Test code may unwrap/expect; the workspace denies these in logic only.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use heapless::Vec as HVec;
use lib_canonical::{CanonicalFault, CanonicalReading, MAX_CELLS, MAX_TEMPS};

/// A `CanonicalReading` filled to `MAX_CELLS`/`MAX_TEMPS` capacity, with every scalar field at an
/// extreme (its type's MIN or MAX), per `extreme`.
fn extreme_full_reading(cell_temp_fill: u16, extreme: Extreme) -> CanonicalReading {
    let mut cell_voltages_mv: HVec<u16, MAX_CELLS> = HVec::new();
    for _ in 0..MAX_CELLS {
        cell_voltages_mv.push(cell_temp_fill).unwrap();
    }
    let mut temperatures_raw: HVec<u16, MAX_TEMPS> = HVec::new();
    for _ in 0..MAX_TEMPS {
        temperatures_raw.push(cell_temp_fill).unwrap();
    }

    let (device_address, current_ca, u16_extreme) = match extreme {
        Extreme::Max => (u8::MAX, i16::MAX, u16::MAX),
        Extreme::Min => (u8::MIN, i16::MIN, u16::MIN),
    };

    CanonicalReading {
        device_address,
        cell_voltages_mv,
        temperatures_raw,
        current_ca,
        total_voltage_cv: u16_extreme,
        remaining_capacity_cah: u16_extreme,
        typical_capacity_cah: u16_extreme,
        rated_capacity_cah: u16_extreme,
        soc_dpct: u16_extreme,
        soh_dpct: u16_extreme,
        cycles: u16_extreme,
        port_voltage_cv: u16_extreme,
    }
}

#[derive(Clone, Copy)]
enum Extreme {
    Max,
    Min,
}

#[test]
fn full_capacity_max_scalars_round_trip() {
    let original = extreme_full_reading(u16::MAX, Extreme::Max);
    assert_eq!(original.cell_voltages_mv.len(), MAX_CELLS);
    assert_eq!(original.temperatures_raw.len(), MAX_TEMPS);

    let mut buf = [0u8; 256];
    let used = postcard::to_slice(&original, &mut buf).expect("encode");
    let (decoded, remainder): (CanonicalReading, &[u8]) =
        postcard::take_from_bytes(used).expect("decode");

    assert_eq!(decoded, original);
    assert!(remainder.is_empty());
}

#[test]
fn full_capacity_min_scalars_round_trip() {
    let original = extreme_full_reading(u16::MIN, Extreme::Min);

    let mut buf = [0u8; 256];
    let used = postcard::to_slice(&original, &mut buf).expect("encode");
    let (decoded, remainder): (CanonicalReading, &[u8]) =
        postcard::take_from_bytes(used).expect("decode");

    assert_eq!(decoded, original);
    assert!(remainder.is_empty());
}

#[test]
fn decode_rejects_every_strict_prefix_of_a_valid_encoding() {
    // Full-capacity reading gives the largest, most representative encoding to truncate.
    let original = extreme_full_reading(u16::MAX, Extreme::Max);

    let mut buf = [0u8; 256];
    let used = postcard::to_slice(&original, &mut buf).expect("encode");
    let n = used.len();
    assert!(n > 1, "sanity: encoding must be more than one byte");

    // Sanity: the full encoding does decode.
    let decoded: CanonicalReading = postcard::from_bytes(&buf[..n]).expect("full decode");
    assert_eq!(decoded, original);

    for len in 0..n {
        assert!(
            postcard::from_bytes::<CanonicalReading>(&buf[..len]).is_err(),
            "expected decode to fail closed on a {len}-byte strict prefix of a {n}-byte encoding"
        );
    }
}

#[test]
fn to_slice_into_too_small_buffer_is_err() {
    let original = extreme_full_reading(u16::MAX, Extreme::Max);

    // First discover the real encoded length, then try one byte short of it.
    let mut probe = [0u8; 256];
    let n = postcard::to_slice(&original, &mut probe)
        .expect("encode")
        .len();

    let mut too_small = vec![0u8; n - 1];
    assert!(postcard::to_slice(&original, &mut too_small).is_err());

    // And the degenerate zero-length buffer.
    let mut zero = [0u8; 0];
    assert!(postcard::to_slice(&original, &mut zero).is_err());
}

#[test]
fn take_from_bytes_with_trailing_bytes_returns_exact_remainder() {
    let original = extreme_full_reading(1234, Extreme::Max);

    let mut buf = [0u8; 300];
    let used_len = postcard::to_slice(&original, &mut buf)
        .expect("encode")
        .len();

    // Append trailing bytes that are NOT part of this message (as if another frame followed it
    // on a shared byte stream) and confirm the remainder is exactly those trailing bytes, no
    // more, no fewer.
    let trailing: &[u8] = &[0xDE, 0xAD, 0xBE, 0xEF, 0x00];
    let mut framed = buf[..used_len].to_vec();
    framed.extend_from_slice(trailing);

    let (decoded, remainder): (CanonicalReading, &[u8]) =
        postcard::take_from_bytes(&framed).expect("decode");

    assert_eq!(decoded, original);
    assert_eq!(remainder, trailing);
}

/// A flag's name paired with a setter that flips exactly that flag on a `CanonicalFault`.
type FaultFlagSetter = (&'static str, fn(&mut CanonicalFault));

/// Every individual protection/warning flag, paired with a setter, so the sweep below can flip
/// exactly one at a time. `device_address` is deliberately excluded — it does not participate in
/// `any()`.
fn fault_flag_setters() -> Vec<FaultFlagSetter> {
    vec![
        ("high_voltage_protection", |f| {
            f.high_voltage_protection = true
        }),
        ("low_voltage_protection", |f| {
            f.low_voltage_protection = true
        }),
        ("high_temperature_charge_protection", |f| {
            f.high_temperature_charge_protection = true
        }),
        ("low_temperature_charge_protection", |f| {
            f.low_temperature_charge_protection = true
        }),
        ("charge_overcurrent_protection", |f| {
            f.charge_overcurrent_protection = true
        }),
        ("discharge_overcurrent_protection", |f| {
            f.discharge_overcurrent_protection = true
        }),
        ("short_circuit_protection", |f| {
            f.short_circuit_protection = true
        }),
        ("mosfet_fault", |f| f.mosfet_fault = true),
        ("cell_balance_warning", |f| f.cell_balance_warning = true),
        ("low_capacity_warning", |f| f.low_capacity_warning = true),
        ("high_temperature_discharge_warning", |f| {
            f.high_temperature_discharge_warning = true
        }),
        ("low_temperature_discharge_warning", |f| {
            f.low_temperature_discharge_warning = true
        }),
        ("low_voltage_warning", |f| f.low_voltage_warning = true),
        ("high_voltage_warning", |f| f.high_voltage_warning = true),
        ("discharge_current_warning", |f| {
            f.discharge_current_warning = true
        }),
        ("charge_current_warning", |f| {
            f.charge_current_warning = true
        }),
    ]
}

#[test]
fn canonical_fault_any_is_true_for_each_flag_set_alone() {
    // Kills dropped-clause mutants in the `any()` OR chain: if any single `||` term were
    // deleted, the corresponding single-flag case here would flip from true to false.
    let setters = fault_flag_setters();
    assert_eq!(
        setters.len(),
        16,
        "sanity: expected all 16 CanonicalFault flags to be covered"
    );

    for (name, set) in setters {
        let mut fault = CanonicalFault::default();
        assert!(!fault.any(), "sanity: default must be all-clear");
        set(&mut fault);
        assert!(fault.any(), "expected any() == true with only `{name}` set");
    }
}

#[test]
fn canonical_fault_any_is_false_with_all_flags_clear() {
    let fault = CanonicalFault {
        device_address: 42, // device_address must not influence any()
        ..CanonicalFault::default()
    };
    assert!(!fault.any());
}
