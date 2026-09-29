//! Native STACK-DEPTH + serialized-SIZE measurement of the broker-thread render/serialize call
//! chain — the dimension Miri cannot model, and the prime remaining lead for the intermittent
//! broker-thread "Illegal load of EXC_RETURN into PC" corruption (Phase-2 plan).
//!
//! On-target evidence: a prior fault showed the broker thread's PSP ~12,768 B PAST a 16,896 B
//! stack, i.e. the render chain tried to use ~29 KB. `render_tms(PowerHardwareUpdate)` →
//! `to_power_hardware` builds a full `PowerHardwareUpdate` (a `Vec<StorageCellState, 50>` stored
//! INLINE in `heapless::Vec`, ~1.2 KiB) and then `serde_json_core::to_slice` monomorphizes a deep
//! recursive serializer over that whole tree — all on the broker thread's stack now.
//!
//! Run: `cargo test -p lib-mapping --test render_stack_depth -- --nocapture --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_mapping::render::{render_fault, render_tms, TmsTopic};
use lib_mapping::schema::{ClockMonotonic, Identity, MAX_CELLS, MAX_TEMPS};
use lib_parse::canonical::{CanonicalFault, CanonicalReading};

/// A reading with `n_cells` cell voltages + `n_temps` temperatures.
fn reading_with(n_cells: u16, n_temps: u16) -> CanonicalReading {
    let mut cells = heapless::Vec::new();
    for i in 0..n_cells {
        cells.push(3800 + i).ok(); // distinct values, defeat constant-folding.
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

// ---------------------------------------------------------------------------------------------
// (A) Serialized-SIZE truth table: what does each TMS payload actually measure, vs MAX_PAYLOAD?
// ---------------------------------------------------------------------------------------------

/// Serialize into a deliberately huge buffer (never the bottleneck) and report the true byte size.
fn serialized_size_power_hardware(n_cells: u16, n_temps: u16) -> usize {
    let mut big = vec![0u8; 64 * 1024];
    let r = reading_with(n_cells, n_temps);
    render_tms(
        TmsTopic::PowerHardwareUpdate,
        device_id(),
        &r,
        now(),
        &mut big,
    )
    .expect("serializes into a 64 KiB buffer")
}

#[test]
fn power_hardware_serialized_size_vs_max_payload() {
    let cap = lib_mapping::sink::MAX_PAYLOAD;
    eprintln!("[size] MAX_PAYLOAD (serialize buffer cap) = {cap} bytes");
    for n in [1u16, 4, 8, 14, 15, MAX_CELLS as u16] {
        let sz = serialized_size_power_hardware(n, MAX_TEMPS as u16);
        let verdict = if sz > cap {
            "OVERFLOWS MAX_PAYLOAD"
        } else {
            "fits"
        };
        eprintln!("[size] PowerHardwareUpdate, {n:2} cells: {sz:5} bytes  -> {verdict}");
    }
    // The device's real batteries report up to MAX_CELLS (16) cells. If a full 16-cell payload
    // exceeds MAX_PAYLOAD, `render_tms` returns BufferTooSmall and the broker's render closure
    // returns None -> the pending publish is silently DROPPED (a quiet-publish cause).
    let full = serialized_size_power_hardware(MAX_CELLS as u16, MAX_TEMPS as u16);
    eprintln!(
        "[size] FULL {}-cell PowerHardwareUpdate = {full} bytes; MAX_PAYLOAD = {cap} \
         (headroom = {})",
        MAX_CELLS,
        cap as isize - full as isize
    );

    // HARD TRIPWIRE: the FULL MAX_CELLS (16-cell) PowerHardwareUpdate MUST fit within MAX_PAYLOAD —
    // real Seplos packs are 16-cell, so an overflow here is not an edge case but a total quiet
    // outage of the topic (measured 1747 B against the old 1664 cap; the 2048 cap was sized from
    // this measurement — see MAX_PAYLOAD's doc).
    assert!(
        full <= cap,
        "{}-cell PowerHardwareUpdate ({} bytes) overflows MAX_PAYLOAD ({} bytes) — \
         every full-pack payload will be silently dropped",
        MAX_CELLS,
        full,
        cap
    );
}

#[test]
fn fault_serialized_size_vs_max_payload() {
    let cap = lib_mapping::sink::MAX_PAYLOAD;
    let mut big = vec![0u8; 64 * 1024];
    let fault = all_flags_fault();
    let sz = render_fault(&fault, device_id(), now(), &mut big).expect("serializes into 64 KiB");
    let verdict = if sz > cap {
        "OVERFLOWS MAX_PAYLOAD"
    } else {
        "fits"
    };
    eprintln!(
        "[size] ActiveDiagnosticState, all 16 flags set: {sz} bytes (cap {cap}) -> {verdict}"
    );
}

// ---------------------------------------------------------------------------------------------
// (B) Native STACK high-water: how deep does the render+serialize chain reach?
// ---------------------------------------------------------------------------------------------

const SENTINEL: u8 = 0xC7;

/// Paint `canvas` with the sentinel, then call `f` (which recurses deeper into the stack, BELOW
/// `canvas`), then count how far down the sentinel was overwritten. `#[inline(never)]` on the two
/// helpers guarantees `f`'s frames nest strictly below `canvas`'s frame rather than beside it.
#[inline(never)]
fn paint_then_run(canvas: &mut [u8], f: impl FnOnce()) -> usize {
    for b in canvas.iter_mut() {
        *b = SENTINEL;
    }
    std::hint::black_box(&mut *canvas);
    f();
    std::hint::black_box(&*canvas);
    // The stack grows DOWN: `canvas[0]` is the highest address (shallowest). `f`'s frames sit below
    // `canvas`, so they dirty the LOW-address end first — walk from the deep end (canvas end) up.
    let mut untouched_deep = 0usize;
    for &b in canvas.iter().rev() {
        if b != SENTINEL {
            break;
        }
        untouched_deep += 1;
    }
    canvas.len() - untouched_deep
}

/// Paint `N` sentinel bytes of a REAL on-stack array (not a heap vec — the array storage must live
/// on the stack for the high-water walk to see the render chain's frames dirtying it), then run the
/// chain deeper, and report how far down the sentinel was overwritten. `PAINT` is a const so the
/// array is stack-resident.
const PAINT: usize = 256 * 1024;

#[inline(never)]
fn paint_array_then_run(f: impl FnOnce()) -> usize {
    let mut canvas = [0u8; PAINT]; // REAL on-stack array (the 8 MiB probe thread has room).
    paint_then_run(&mut canvas, f)
}

/// Run the render chain on a generous child stack, painting a probe region ABOVE it, and report how
/// many bytes below the probe the chain reached — its native stack high-water mark on this host.
fn measure_high_water(label: &str, chain: fn(&mut [u8]) -> usize) -> usize {
    const PROBE_STACK: usize = 8 * 1024 * 1024;

    let handle = std::thread::Builder::new()
        .stack_size(PROBE_STACK)
        .spawn(move || -> usize {
            paint_array_then_run(|| {
                // Heap `out` (like the real broker's static `PUMP_OUT_BUF`) so the high-water
                // reading reflects ONLY the render chain's own frames, not the output buffer.
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

fn chain_power_hardware(out: &mut [u8]) -> usize {
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

fn chain_dc_measurement(out: &mut [u8]) -> usize {
    use std::hint::black_box;
    let r = black_box(reading_with(MAX_CELLS as u16, MAX_TEMPS as u16));
    render_tms(
        TmsTopic::DcMeasurementUpdate,
        black_box(device_id()),
        black_box(&r),
        black_box(now()),
        out,
    )
    .unwrap_or(0)
}

fn chain_fault(out: &mut [u8]) -> usize {
    use std::hint::black_box;
    let f = black_box(all_flags_fault());
    render_fault(black_box(&f), black_box(device_id()), black_box(now()), out).unwrap_or(0)
}

#[test]
fn render_native_stack_high_water_all_topics() {
    eprintln!(
        "[stack] NOTE: host x86_64 measurement is a FLOOR for the thumbv7 broker-thread requirement \
         (different ABI, no red-zone, but same struct-build + serde monomorphization shape)."
    );
    let ph = measure_high_water(
        "render_tms(PowerHardwareUpdate)+serialize",
        chain_power_hardware,
    );
    let dc = measure_high_water(
        "render_tms(DcMeasurementUpdate)+serialize",
        chain_dc_measurement,
    );
    let ft = measure_high_water("render_fault(ActiveDiagnosticState)+serialize", chain_fault);
    let worst = ph.max(dc).max(ft);
    eprintln!(
        "[stack] WORST-CASE render chain high-water (host floor) = {worst} bytes ({} KiB)",
        worst / 1024
    );
    assert!(worst < 8 * 1024 * 1024, "implausible measurement {worst}");
}

/// Confirm the render chain SURVIVES on a stack the size of the broker thread's configured stack,
/// and probe a few smaller sizes to bracket the true minimum (each on its own thread; a size the
/// chain cannot survive aborts its child — reported as a non-join).
#[test]
fn render_chain_survives_broker_stack_sizes() {
    // Candidate broker-thread stack sizes to bracket (the fault was observed at 16,896 B).
    for stack in [8 * 1024usize, 12 * 1024, 16 * 1024, 24 * 1024, 32 * 1024] {
        let survived = std::thread::Builder::new()
            .stack_size(stack)
            .spawn(move || {
                // Heap-allocate `out` so ONLY the render chain's own frames count against the
                // capped stack (a stack-local 64 KiB buffer would itself blow every small cap and
                // ruin the measurement). On-target the broker's `out` is `static PUMP_OUT_BUF`,
                // likewise off-stack — so heap here matches the real memory placement.
                let mut out = vec![0u8; 64 * 1024];
                let n = chain_power_hardware(&mut out);
                std::hint::black_box(n);
            })
            .map(|h| h.join().is_ok())
            .unwrap_or(false);
        eprintln!(
            "[stack] render_tms(PowerHardwareUpdate) on {stack:6}-byte stack: survived={survived}"
        );
    }
}
