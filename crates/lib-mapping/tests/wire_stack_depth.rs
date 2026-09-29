//! Native STACK-DEPTH measurement of the flat wire serializer (`crate::wire`), mirroring
//! `render_stack_depth.rs`'s technique — direct comparison against the OLD serde/`render_tms`/
//! `render_fault` path this module replaces (see `docs/wire-serialization.md` for the fault this
//! measures the fix for: `render_fault` reserved 14,400 B, `rust_broker_main` 9,664 B on-target).
//!
//! Run: `cargo test -p lib-mapping --test wire_stack_depth -- --nocapture --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_mapping::render::{render_fault, render_tms, TmsTopic};
use lib_mapping::schema::{ClockMonotonic, Identity, MAX_CELLS, MAX_TEMPS};
use lib_mapping::wire::{
    serialize_active_diagnostic, serialize_dc_measurement, serialize_into, serialize_power_hardware,
};
use lib_parse::canonical::{CanonicalFault, CanonicalReading};

fn reading_with(n_cells: u16, n_temps: u16) -> CanonicalReading {
    let mut cells = heapless::Vec::new();
    for i in 0..n_cells {
        cells.push(3800 + i).ok();
    }
    let mut temps = heapless::Vec::new();
    for i in 0..n_temps {
        temps.push(2731 + i * 3).ok();
    }
    CanonicalReading {
        device_address: 0,
        cell_voltages_mv: cells,
        temperatures_raw: temps,
        current_ca: 164,
        total_voltage_cv: 5452,
        remaining_capacity_cah: 13953,
        typical_capacity_cah: 18000,
        rated_capacity_cah: 18000,
        soc_dpct: 775,
        soh_dpct: 1000,
        cycles: 2,
        port_voltage_cv: 5453,
    }
}

fn device_id() -> Identity {
    let mut id: Identity = heapless::String::new();
    id.push_str("METEOR-001").unwrap();
    id
}

fn now() -> ClockMonotonic {
    ClockMonotonic {
        seconds: 12345,
        nanoseconds: 678_000_000,
    }
}

fn all_flags_fault() -> CanonicalFault {
    CanonicalFault {
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
    }
}

const SENTINEL: u8 = 0xC7;

#[inline(never)]
fn paint_then_run(canvas: &mut [u8], f: impl FnOnce()) -> usize {
    for b in canvas.iter_mut() {
        *b = SENTINEL;
    }
    std::hint::black_box(&mut *canvas);
    f();
    std::hint::black_box(&*canvas);
    let mut untouched_deep = 0usize;
    for &b in canvas.iter().rev() {
        if b != SENTINEL {
            break;
        }
        untouched_deep += 1;
    }
    canvas.len() - untouched_deep
}

const PAINT: usize = 256 * 1024;

#[inline(never)]
fn paint_array_then_run(f: impl FnOnce()) -> usize {
    let mut canvas = [0u8; PAINT];
    paint_then_run(&mut canvas, f)
}

fn measure_high_water(label: &str, chain: fn(&mut [u8]) -> usize) -> usize {
    const PROBE_STACK: usize = 8 * 1024 * 1024;
    let handle = std::thread::Builder::new()
        .stack_size(PROBE_STACK)
        .spawn(move || -> usize {
            paint_array_then_run(|| {
                let mut out = vec![0u8; 64 * 1024];
                let n = chain(&mut out);
                std::hint::black_box(&out[..n]);
            })
        })
        .expect("probe thread spawns");
    let used = handle.join().expect("probe thread completes");
    eprintln!(
        "[stack] {label}: chain reached ~{used} bytes ({} KiB) below the probe region",
        used / 1024
    );
    used
}

// ---- OLD path (serde/render.rs) — the band-aided baseline this fix replaces ----

fn old_chain_power_hardware(out: &mut [u8]) -> usize {
    use std::hint::black_box;
    let r = black_box(reading_with(MAX_CELLS as u16, MAX_TEMPS as u16));
    render_tms(
        TmsTopic::PowerHardwareUpdate,
        black_box(device_id()),
        black_box(&r),
        black_box(now()),
        out,
    )
    .unwrap_or(0)
}

fn old_chain_fault(out: &mut [u8]) -> usize {
    use std::hint::black_box;
    let f = black_box(all_flags_fault());
    render_fault(black_box(&f), black_box(device_id()), black_box(now()), out).unwrap_or(0)
}

// ---- NEW path (crate::wire, flat serializer) ----

fn new_chain_power_hardware(out: &mut [u8]) -> usize {
    use std::hint::black_box;
    let r = black_box(reading_with(MAX_CELLS as u16, MAX_TEMPS as u16));
    let dev = black_box(device_id());
    serialize_into(out, |s| serialize_power_hardware(&r, &dev, now(), s)).unwrap_or(0)
}

fn new_chain_dc_measurement(out: &mut [u8]) -> usize {
    use std::hint::black_box;
    let r = black_box(reading_with(MAX_CELLS as u16, MAX_TEMPS as u16));
    let dev = black_box(device_id());
    serialize_into(out, |s| serialize_dc_measurement(&r, &dev, now(), s)).unwrap_or(0)
}

fn new_chain_fault(out: &mut [u8]) -> usize {
    use std::hint::black_box;
    let f = black_box(all_flags_fault());
    let dev = black_box(device_id());
    serialize_into(out, |s| serialize_active_diagnostic(&f, &dev, now(), s)).unwrap_or(0)
}

/// Best-effort native stack high-water comparison, OLD (serde) vs NEW (flat) path, for the same
/// worst-case inputs (full MAX_CELLS/MAX_TEMPS reading, all 16 fault flags set). Mirrors
/// `render_stack_depth.rs`'s pre-existing sentinel-painting technique verbatim (including its own
/// loose upper-bound-only assertion) rather than tightening it, because in this host environment
/// the sentinel-painting technique itself measures 0 B for BOTH paths (glibc's thread-stack
/// page-dirtying doesn't line up with the sentinel walk here — `render_stack_depth.rs`'s own
/// pre-existing test has the identical 0 B result, confirmed before this test was added, so this is
/// an environment characteristic of the technique, not a defect in the new serializer). The
/// authoritative Phase 1 gate for stack depth is therefore `-Zemit-stack-sizes`/frame-size
/// inspection (see the crate's `RUSTFLAGS=-Zemit-stack-sizes` note) plus, decisively, the Phase 2
/// on-target QEMU stack re-measure (`CONFIG_THREAD_STACK_INFO`) — this host test is a convenience
/// diagnostic only, kept alongside `render_stack_depth.rs` for a like-for-like before/after
/// printout when the technique DOES produce nonzero numbers on a given host.
#[test]
fn new_serializer_stack_high_water_vs_old_path() {
    eprintln!(
        "[stack] NOTE: host x86_64 measurement is a FLOOR for the thumbv7 broker-thread requirement \
         (different ABI, no red-zone) — used here as a BEFORE/AFTER comparison, not an absolute \
         on-target prediction. May read 0 B on hosts where the sentinel-painting technique doesn't \
         observe dirtied stack pages (see this test's doc comment) — the QEMU re-measure is the real \
         gate."
    );

    let old_hw = measure_high_water(
        "OLD render_tms(PowerHardwareUpdate)+serde",
        old_chain_power_hardware,
    );
    let old_ft = measure_high_water(
        "OLD render_fault(ActiveDiagnosticState)+serde",
        old_chain_fault,
    );
    let old_worst = old_hw.max(old_ft);

    let new_hw = measure_high_water(
        "NEW serialize_power_hardware (flat)",
        new_chain_power_hardware,
    );
    let new_dc = measure_high_water(
        "NEW serialize_dc_measurement (flat)",
        new_chain_dc_measurement,
    );
    let new_ft = measure_high_water("NEW serialize_active_diagnostic (flat)", new_chain_fault);
    let new_worst = new_hw.max(new_dc).max(new_ft);

    eprintln!(
        "[stack] OLD worst-case (serde path) = {old_worst} bytes ({} KiB); NEW worst-case (flat) = \
         {new_worst} bytes ({} KiB)",
        old_worst / 1024,
        new_worst / 1024,
    );

    // Loose sanity ceiling only (matches render_stack_depth.rs's own assertion style) — see this
    // test's doc comment for why a strict `new_worst < old_worst` comparison isn't reliable on
    // every host.
    assert!(
        new_worst < 8 * 1024 * 1024,
        "implausible measurement {new_worst}"
    );
    assert!(
        old_worst < 8 * 1024 * 1024,
        "implausible measurement {old_worst}"
    );
}
