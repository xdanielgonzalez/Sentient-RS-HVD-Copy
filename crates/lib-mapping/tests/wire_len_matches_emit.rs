//! THE critical safety invariant for the flat wire serializer (`crate::wire`): `serialized_len()`
//! (driven against `CountingSink`) must equal the byte count `serialize_into()` (driven against
//! `WriteSink`) actually writes, for every input — a mismatch would corrupt the MQTT PUBLISH length
//! prefix (the length is written to the fixed header before the payload bytes are streamed). Both
//! functions run the exact same serialize closure (see `wire/mod.rs`'s "one shared traversal" doc
//! comment) so this is a proof of that design, not an incidental property.
//!
//! Property-tested over randomized `CanonicalReading`/`CanonicalFault` inputs: varying cell counts
//! (0..=MAX_CELLS), temp counts (0..=MAX_TEMPS), all scalar ranges including negatives (current) and
//! sub-1.0 values (soc), and every fault-flag combination.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_canonical::{MAX_CELLS, MAX_TEMPS};
use lib_mapping::wire::{
    serialize_active_diagnostic, serialize_dc_measurement, serialize_into,
    serialize_power_hardware, serialized_len, CountingSink, JsonSink, WriteSink,
};
use lib_parse::canonical::{CanonicalFault, CanonicalReading};
use lib_tms::{ClockMonotonic, Identity};
use proptest::prelude::*;

fn device_id() -> Identity {
    let mut id = Identity::new();
    id.push_str("METEOR-001").unwrap();
    id
}

/// Big enough for the largest payload this crate ever produces (a full MAX_CELLS/MAX_TEMPS reading
/// or a full MAX_DTC fault list), with generous headroom — the point of this test is the
/// counted-vs-emitted invariant, not exercising the buffer-too-small path (that's a separate test).
const BIG: usize = 16 * 1024;

fn arb_reading() -> impl Strategy<Value = CanonicalReading> {
    (
        any::<u8>(),
        proptest::collection::vec(any::<u16>(), 0..=MAX_CELLS),
        proptest::collection::vec(any::<u16>(), 0..=MAX_TEMPS),
        any::<i16>(),
        any::<u16>(),
        any::<u16>(),
        any::<u16>(),
        any::<u16>(),
        any::<u16>(),
        any::<u16>(),
        any::<u16>(),
        any::<u16>(),
    )
        .prop_map(
            |(
                device_address,
                cells,
                temps,
                current_ca,
                total_voltage_cv,
                remaining_capacity_cah,
                typical_capacity_cah,
                rated_capacity_cah,
                soc_dpct,
                soh_dpct,
                cycles,
                port_voltage_cv,
            )| {
                let mut cell_voltages_mv = heapless::Vec::new();
                for c in cells {
                    let _ = cell_voltages_mv.push(c);
                }
                let mut temperatures_raw = heapless::Vec::new();
                for t in temps {
                    let _ = temperatures_raw.push(t);
                }
                CanonicalReading {
                    device_address,
                    cell_voltages_mv,
                    temperatures_raw,
                    current_ca,
                    total_voltage_cv,
                    remaining_capacity_cah,
                    typical_capacity_cah,
                    rated_capacity_cah,
                    soc_dpct,
                    soh_dpct,
                    cycles,
                    port_voltage_cv,
                }
            },
        )
}

/// `CanonicalFault` has 16 boolean flags + a `u8` address (17 fields total) — past `proptest`'s
/// 12-element tuple `Strategy` impl, so the 16 flags are generated as one `u16` bitmask and unpacked
/// bit-by-bit rather than as 16 separate `any::<bool>()` tuple slots.
fn arb_fault() -> impl Strategy<Value = CanonicalFault> {
    (any::<u8>(), any::<u16>()).prop_map(|(device_address, flags)| CanonicalFault {
        device_address,
        high_voltage_protection: flags & (1 << 0) != 0,
        low_voltage_protection: flags & (1 << 1) != 0,
        high_temperature_charge_protection: flags & (1 << 2) != 0,
        low_temperature_charge_protection: flags & (1 << 3) != 0,
        charge_overcurrent_protection: flags & (1 << 4) != 0,
        discharge_overcurrent_protection: flags & (1 << 5) != 0,
        short_circuit_protection: flags & (1 << 6) != 0,
        mosfet_fault: flags & (1 << 7) != 0,
        cell_balance_warning: flags & (1 << 8) != 0,
        low_capacity_warning: flags & (1 << 9) != 0,
        high_temperature_discharge_warning: flags & (1 << 10) != 0,
        low_temperature_discharge_warning: flags & (1 << 11) != 0,
        low_voltage_warning: flags & (1 << 12) != 0,
        high_voltage_warning: flags & (1 << 13) != 0,
        discharge_current_warning: flags & (1 << 14) != 0,
        charge_current_warning: flags & (1 << 15) != 0,
    })
}

fn now_arb() -> impl Strategy<Value = ClockMonotonic> {
    (any::<u32>(), any::<u32>()).prop_map(|(seconds, nanoseconds)| ClockMonotonic {
        seconds,
        nanoseconds,
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// `serialized_len` (CountingSink) == the byte count `serialize_into` (WriteSink) actually
    /// writes, for `serialize_dc_measurement`, over randomized readings/clocks.
    #[test]
    fn dc_measurement_counted_len_matches_emitted_len(
        reading in arb_reading(),
        now in now_arb(),
    ) {
        let dev = device_id();
        let counted = serialized_len(|s| serialize_dc_measurement(&reading, &dev, now, s));
        let mut buf = [0u8; BIG];
        let emitted = serialize_into(&mut buf, |s| serialize_dc_measurement(&reading, &dev, now, s))
            .expect("fits in BIG");
        prop_assert_eq!(counted, emitted);
        // Also confirm the emitted bytes are valid UTF-8 JSON (parses cleanly) as a sanity check
        // that "same length" isn't masking "both wrong the same way".
        let s = core::str::from_utf8(&buf[..emitted]).expect("valid utf8");
        let _: serde_json::Value = serde_json::from_str(s).expect("valid JSON");
    }

    /// Same invariant for `serialize_power_hardware` (the largest payload — up to MAX_CELLS cells).
    #[test]
    fn power_hardware_counted_len_matches_emitted_len(
        reading in arb_reading(),
        now in now_arb(),
    ) {
        let dev = device_id();
        let counted = serialized_len(|s| serialize_power_hardware(&reading, &dev, now, s));
        let mut buf = [0u8; BIG];
        let emitted = serialize_into(&mut buf, |s| serialize_power_hardware(&reading, &dev, now, s))
            .expect("fits in BIG");
        prop_assert_eq!(counted, emitted);
        let s = core::str::from_utf8(&buf[..emitted]).expect("valid utf8");
        let _: serde_json::Value = serde_json::from_str(s).expect("valid JSON");
    }

    /// Same invariant for `serialize_active_diagnostic`, over every fault-flag combination.
    #[test]
    fn active_diagnostic_counted_len_matches_emitted_len(
        fault in arb_fault(),
        now in now_arb(),
    ) {
        let dev = device_id();
        let counted = serialized_len(|s| serialize_active_diagnostic(&fault, &dev, now, s));
        let mut buf = [0u8; BIG];
        let emitted = serialize_into(&mut buf, |s| serialize_active_diagnostic(&fault, &dev, now, s))
            .expect("fits in BIG");
        prop_assert_eq!(counted, emitted);
        let s = core::str::from_utf8(&buf[..emitted]).expect("valid utf8");
        let _: serde_json::Value = serde_json::from_str(s).expect("valid JSON");
    }

    /// Same counted-vs-emitted invariant for `JsonSink::f32` directly (no `serialize_*` function in
    /// this crate emits `f32` today — see `wire/mod.rs`'s module doc comment — but the invariant must
    /// hold for the sink methods themselves, for whichever future encoder calls them). `any::<f32>()`
    /// covers the full bit range, including `NaN`/`±Infinity` (-> `null`, see `JsonSink::f32`'s doc
    /// comment).
    #[test]
    fn f32_counted_len_matches_emitted_len(v in any::<f32>()) {
        let mut counting = CountingSink::new();
        counting.f32(v);
        let mut buf = [0u8; 64];
        let mut writer = WriteSink::new(&mut buf);
        writer.f32(v);
        prop_assert!(!writer.overflowed());
        prop_assert_eq!(counting.len(), writer.len());
    }

    /// `f64` counterpart of `f32_counted_len_matches_emitted_len`.
    #[test]
    fn f64_counted_len_matches_emitted_len(v in any::<f64>()) {
        let mut counting = CountingSink::new();
        counting.f64(v);
        let mut buf = [0u8; 64];
        let mut writer = WriteSink::new(&mut buf);
        writer.f64(v);
        prop_assert!(!writer.overflowed());
        prop_assert_eq!(counting.len(), writer.len());
    }
}

/// A too-small buffer reports cleanly via `None` (not a panic, not an out-of-bounds write, not a
/// truncated/corrupt result) for all three serialize functions.
#[test]
fn buffer_too_small_returns_none_cleanly() {
    let dev = device_id();
    let now = ClockMonotonic {
        seconds: 0,
        nanoseconds: 0,
    };
    let mut reading = CanonicalReading {
        device_address: 0,
        cell_voltages_mv: heapless::Vec::new(),
        temperatures_raw: heapless::Vec::new(),
        current_ca: -164,
        total_voltage_cv: 5452,
        remaining_capacity_cah: 0,
        typical_capacity_cah: 0,
        rated_capacity_cah: 0,
        soc_dpct: 775,
        soh_dpct: 0,
        cycles: 2,
        port_voltage_cv: 0,
    };
    for i in 0..MAX_CELLS as u16 {
        reading.cell_voltages_mv.push(3800 + i).unwrap();
    }
    for i in 0..MAX_TEMPS as u16 {
        reading.temperatures_raw.push(2981 + i * 2).unwrap();
    }
    let fault = CanonicalFault {
        device_address: 0,
        high_voltage_protection: true,
        low_voltage_protection: true,
        high_temperature_charge_protection: true,
        low_temperature_charge_protection: true,
        charge_overcurrent_protection: true,
        discharge_overcurrent_protection: true,
        short_circuit_protection: true,
        mosfet_fault: true,
        cell_balance_warning: true,
        low_capacity_warning: true,
        high_temperature_discharge_warning: true,
        low_temperature_discharge_warning: true,
        low_voltage_warning: true,
        high_voltage_warning: true,
        discharge_current_warning: true,
        charge_current_warning: true,
    };

    // Zero-length and tiny buffers must all cleanly report None, for every topic.
    for cap in [0usize, 1, 4, 16] {
        let mut buf = vec![0u8; cap];
        assert!(
            serialize_into(&mut buf, |s| serialize_dc_measurement(
                &reading, &dev, now, s
            ))
            .is_none(),
            "cap={cap} dc_measurement"
        );
        assert!(
            serialize_into(&mut buf, |s| serialize_power_hardware(
                &reading, &dev, now, s
            ))
            .is_none(),
            "cap={cap} power_hardware"
        );
        assert!(
            serialize_into(&mut buf, |s| serialize_active_diagnostic(
                &fault, &dev, now, s
            ))
            .is_none(),
            "cap={cap} active_diagnostic"
        );
    }

    // A buffer exactly `serialized_len()` bytes must succeed; one byte short must fail (fence-post).
    let len = serialized_len(|s| serialize_power_hardware(&reading, &dev, now, s));
    let mut exact = vec![0u8; len];
    assert_eq!(
        serialize_into(&mut exact, |s| serialize_power_hardware(
            &reading, &dev, now, s
        )),
        Some(len)
    );
    let mut short = vec![0u8; len - 1];
    assert!(serialize_into(&mut short, |s| serialize_power_hardware(
        &reading, &dev, now, s
    ))
    .is_none());
}
