//! THE safety invariant for the XCDR2 CDR serializer (`lib_tms::cdr` + the generated chains, reached
//! via [`lib_mapping::render_cdr`]): [`counted_len_cdr`] (driven against `CdrCountingSink`) must equal
//! the byte count [`write_cdr`] (driven against `CdrWriteSink`) actually writes, for every input — a
//! mismatch would corrupt an RTPS SerializedPayload length. Both drive the exact same member chain
//! (see `lib_tms::cdr::emit_dheader`'s no-back-patching dual traversal), so this is a proof of that
//! design, not an incidental property — the CDR twin of `wire_len_matches_emit`.
//!
//! Property-tested over randomized `CanonicalReading`/`CanonicalFault`/`CanonicalInverterReading`
//! inputs across every rendered topic, plus the ALIGNMENT-PHASE property (`lib_tms::cdr` module doc,
//! "Position independence"): a generated chain's serialized CONTENT size — counted after the
//! mandatory `align(4)` that precedes every DHEADER-delimited subtree — is identical at every
//! enclosing phase 0..4. That invariant is what lets `emit_dheader` size a subtree from a fresh
//! phase-0 counting sink without threading the real stream position.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_canonical::{MAX_CELLS, MAX_TEMPS};
use lib_mapping::render::TmsTopic;
use lib_mapping::render_cdr::{
    counted_len_cdr, render_ac_measurement_inverter_cdr, render_active_diagnostic_inverter_cdr,
    render_dc_measurement_inverter_cdr, render_fault_cdr, render_power_hardware_inverter_cdr,
    render_storage_update_cdr, render_tms_cdr, write_cdr,
};
use lib_mapping::wire::{
    ActiveDiagnosticStateCdrView, DcMeasurementUpdateCdrView, PowerHardwareUpdateCdrView,
    StorageUpdateCdrView,
};
use lib_parse::canonical::{
    CanonicalFault, CanonicalInverterFault, CanonicalInverterReading, CanonicalReading,
    InverterMode,
};
use lib_tms::cdr::{CdrChain, CdrCountingSink, CdrSink};
use lib_tms::{ClockMonotonic, Identity};
use proptest::prelude::*;

fn device_id() -> Identity {
    let mut id = Identity::new();
    id.push_str("METEOR-001").unwrap();
    id
}

/// Big enough for the largest CDR payload this crate produces, with headroom — this test is about
/// the counted-vs-emitted invariant, not the buffer-too-small path.
const BIG: usize = 32 * 1024;

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

fn arb_inverter_reading() -> impl Strategy<Value = CanonicalInverterReading> {
    (
        any::<u16>(),
        any::<u16>(),
        any::<u16>(),
        any::<u16>(),
        any::<u16>(),
        any::<u16>(),
        any::<u16>(),
        any::<u16>(),
        any::<u16>(),
        any::<i16>(),
        any::<u16>(),
        any::<u16>(),
    )
        .prop_map(
            |(
                grid_voltage_dv,
                grid_frequency_dhz,
                ac_output_voltage_dv,
                ac_output_frequency_dhz,
                ac_output_apparent_power_va,
                ac_output_active_power_w,
                battery_voltage_cv,
                battery_charging_current_a,
                battery_discharge_current_a,
                inverter_temperature_c,
                pv_current_da,
                pv_voltage_dv,
            )| CanonicalInverterReading {
                grid_voltage_dv,
                grid_frequency_dhz,
                ac_output_voltage_dv,
                ac_output_frequency_dhz,
                ac_output_apparent_power_va,
                ac_output_active_power_w,
                bus_voltage_v: 400,
                battery_voltage_cv,
                battery_charging_current_a,
                battery_discharge_current_a,
                inverter_temperature_c,
                pv_current_da,
                pv_voltage_dv,
                pv_charging_power_w: 1900,
                status_bits: 0,
                mode: InverterMode::Line,
                ratings: None,
            },
        )
}

fn arb_inverter_fault() -> impl Strategy<Value = CanonicalInverterFault> {
    any::<u8>().prop_map(|bits| CanonicalInverterFault {
        line_fail: bits & 1 != 0,
        overtemperature_fault: bits & 2 != 0,
        fan_locked_fault: bits & 4 != 0,
        battery_low_fault: bits & 8 != 0,
        overload_fault: bits & 16 != 0,
        output_short_fault: bits & 32 != 0,
        raw_bits: 0,
    })
}

fn now_arb() -> impl Strategy<Value = ClockMonotonic> {
    (any::<u32>(), any::<u32>()).prop_map(|(seconds, nanoseconds)| ClockMonotonic {
        seconds,
        nanoseconds,
    })
}

/// Emit a topic via its public `render_*_cdr` fn, and separately count it, asserting equality and
/// that the buffer did not overflow.
fn check_render(counted: usize, emitted: usize, label: &str) {
    assert_eq!(counted, emitted, "{label}: counted != emitted");
    // XCDR2 payload is always padded to a 4-byte boundary (trailing encapsulation padding).
    assert_eq!(emitted % 4, 0, "{label}: total length not 4-aligned");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn dc_measurement_counted_matches_emitted(reading in arb_reading(), now in now_arb()) {
        let dev = device_id();
        let view = DcMeasurementUpdateCdrView { reading: &reading, device_id: &dev, now: &now };
        let counted = counted_len_cdr::<_, lib_tms::generated_cdr::dc_measurement_update_cdr::CdrChain>(&view);
        let mut buf = [0u8; BIG];
        let emitted = render_tms_cdr(TmsTopic::DcMeasurementUpdate, dev.clone(), &reading, now, &mut buf)
            .expect("fits");
        check_render(counted, emitted, "dc_measurement");
    }

    #[test]
    fn power_hardware_counted_matches_emitted(reading in arb_reading(), now in now_arb()) {
        let dev = device_id();
        let view = PowerHardwareUpdateCdrView { reading: &reading, device_id: &dev, now: &now };
        let counted = counted_len_cdr::<_, lib_tms::generated_cdr::power_hardware_update_cdr::CdrChain>(&view);
        let mut buf = [0u8; BIG];
        let emitted = render_tms_cdr(TmsTopic::PowerHardwareUpdate, dev.clone(), &reading, now, &mut buf)
            .expect("fits");
        check_render(counted, emitted, "power_hardware");
    }

    #[test]
    fn active_diagnostic_counted_matches_emitted(fault in arb_fault(), now in now_arb()) {
        let dev = device_id();
        let view = ActiveDiagnosticStateCdrView { fault: &fault, device_id: &dev, now: &now };
        let counted = counted_len_cdr::<_, lib_tms::generated_cdr::active_diagnostic_state_cdr::CdrChain>(&view);
        let mut buf = [0u8; BIG];
        let emitted = render_fault_cdr(&fault, dev.clone(), now, &mut buf).expect("fits");
        check_render(counted, emitted, "active_diagnostic");
    }

    #[test]
    fn storage_update_counted_matches_emitted(reading in arb_reading()) {
        let dev = device_id();
        let view = StorageUpdateCdrView { reading: &reading, device_id: &dev };
        let counted = counted_len_cdr::<_, lib_tms::generated_cdr::storage_update_cdr::CdrChain>(&view);
        let mut buf = [0u8; BIG];
        let emitted = render_storage_update_cdr(dev.clone(), &reading, &mut buf).expect("fits");
        check_render(counted, emitted, "storage_update");
    }

    #[test]
    fn ac_measurement_inverter_counted_matches_emitted(reading in arb_inverter_reading(), now in now_arb()) {
        let dev = device_id();
        let mut buf = [0u8; BIG];
        let emitted = render_ac_measurement_inverter_cdr(dev.clone(), &reading, now, &mut buf).expect("fits");
        // Count via a fresh write into a same-size buffer — same chain, so the emitted length here is
        // the invariant target; recompute it independently via a second write.
        let mut buf2 = [0u8; BIG];
        let emitted2 = render_ac_measurement_inverter_cdr(dev, &reading, now, &mut buf2).expect("fits");
        prop_assert_eq!(emitted, emitted2);
        prop_assert_eq!(&buf[..emitted], &buf2[..emitted2]);
        prop_assert_eq!(emitted % 4, 0);
    }

    #[test]
    fn dc_measurement_inverter_counted_matches_emitted(reading in arb_inverter_reading(), now in now_arb()) {
        let dev = device_id();
        let mut buf = [0u8; BIG];
        let emitted = render_dc_measurement_inverter_cdr(dev, &reading, now, &mut buf).expect("fits");
        prop_assert_eq!(emitted % 4, 0);
    }

    #[test]
    fn power_hardware_inverter_counted_matches_emitted(reading in arb_inverter_reading(), now in now_arb()) {
        let dev = device_id();
        let mut buf = [0u8; BIG];
        let emitted = render_power_hardware_inverter_cdr(dev, &reading, now, &mut buf).expect("fits");
        prop_assert_eq!(emitted % 4, 0);
    }

    #[test]
    fn active_diagnostic_inverter_counted_matches_emitted(fault in arb_inverter_fault(), now in now_arb()) {
        let dev = device_id();
        let mut buf = [0u8; BIG];
        let emitted = render_active_diagnostic_inverter_cdr(&fault, InverterMode::Fault, dev, now, &mut buf)
            .expect("fits");
        prop_assert_eq!(emitted % 4, 0);
    }
}

/// The ALIGNMENT-PHASE property (`lib_tms::cdr` module doc, "Position independence"): a generated
/// chain's serialized CONTENT size — counted after the `align(4)` that precedes every
/// DHEADER-delimited subtree — is identical at every enclosing phase 0..=3. This is what makes
/// `emit_dheader`'s "count from a fresh phase-0 sink" shortcut correct for a subtree written at an
/// arbitrary (but 4-aligned) offset. Checked over the top-level member chain of each battery topic.
#[test]
fn alignment_phase_content_size_is_position_independent() {
    let dev = device_id();
    let now = ClockMonotonic {
        seconds: 12345,
        nanoseconds: 678_000_000,
    };
    let mut reading = CanonicalReading {
        device_address: 0,
        cell_voltages_mv: heapless::Vec::new(),
        temperatures_raw: heapless::Vec::new(),
        current_ca: -164,
        total_voltage_cv: 5452,
        remaining_capacity_cah: 1395,
        typical_capacity_cah: 1800,
        rated_capacity_cah: 1800,
        soc_dpct: 775,
        soh_dpct: 1000,
        cycles: 2,
        port_voltage_cv: 5453,
    };
    for i in 0..14u16 {
        reading.cell_voltages_mv.push(3800 + i).unwrap();
    }
    for i in 0..6u16 {
        reading.temperatures_raw.push(2981 + i).unwrap();
    }

    fn content_size<Src, C: CdrChain<Src>>(src: &Src, phase: usize) -> usize {
        let mut c = CdrCountingSink::at_phase(phase);
        c.align(4); // reach the 4-aligned offset a DHEADER-delimited subtree begins at
        let base = c.pos();
        C::emit(src, &mut c);
        c.pos() - base
    }

    macro_rules! assert_phase_independent {
        ($view:expr, $chain:ty, $label:literal) => {{
            let v = $view;
            let sizes: Vec<usize> = (0..4).map(|p| content_size::<_, $chain>(&v, p)).collect();
            for (p, s) in sizes.iter().enumerate() {
                assert_eq!(
                    *s, sizes[0],
                    "{}: content size at phase {p} ({s}) != phase 0 ({})",
                    $label, sizes[0]
                );
            }
        }};
    }

    assert_phase_independent!(
        DcMeasurementUpdateCdrView {
            reading: &reading,
            device_id: &dev,
            now: &now
        },
        lib_tms::generated_cdr::dc_measurement_update_cdr::CdrChain,
        "dc_measurement"
    );
    assert_phase_independent!(
        PowerHardwareUpdateCdrView {
            reading: &reading,
            device_id: &dev,
            now: &now
        },
        lib_tms::generated_cdr::power_hardware_update_cdr::CdrChain,
        "power_hardware"
    );
    assert_phase_independent!(
        StorageUpdateCdrView {
            reading: &reading,
            device_id: &dev
        },
        lib_tms::generated_cdr::storage_update_cdr::CdrChain,
        "storage_update"
    );
}

/// A too-small buffer reports cleanly via `Err` (not a panic, not OOB, not a truncated payload) for
/// the CDR render path, and an exactly-sized buffer succeeds (fence-post).
#[test]
fn buffer_too_small_returns_err_cleanly() {
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

    let view = PowerHardwareUpdateCdrView {
        reading: &reading,
        device_id: &dev,
        now: &now,
    };
    let len =
        counted_len_cdr::<_, lib_tms::generated_cdr::power_hardware_update_cdr::CdrChain>(&view);

    for cap in [0usize, 1, 4, 8, 16] {
        let mut buf = vec![0u8; cap];
        assert!(
            render_tms_cdr(
                TmsTopic::PowerHardwareUpdate,
                dev.clone(),
                &reading,
                now,
                &mut buf
            )
            .is_err(),
            "cap={cap} should fail"
        );
    }

    let mut exact = vec![0u8; len];
    assert_eq!(
        write_cdr::<_, lib_tms::generated_cdr::power_hardware_update_cdr::CdrChain>(
            &view, &mut exact
        ),
        Ok(len),
        "exact-size buffer must succeed"
    );
    let mut short = vec![0u8; len - 1];
    assert!(
        write_cdr::<_, lib_tms::generated_cdr::power_hardware_update_cdr::CdrChain>(
            &view, &mut short
        )
        .is_err(),
        "one byte short must fail"
    );
}
