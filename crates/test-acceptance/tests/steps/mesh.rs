//! Steps for the Meshtastic mesh-radio secondary publish target feature area — see
//! `docs/gap-analysis-meteor-parity.md`'s "Meshtastic mesh-radio forwarding" finding.
//!
//! GREP-GATE — no re-implemented condensation logic lives here. The one call under test is the
//! real, reused `lib_net::mesh::condense` (the greedy priority-ordered JSON packer); this file is
//! Gherkin<->Rust plumbing only: staging a `CanonicalReading`/`LocationFix` fixture, calling
//! `condense`, and asserting on the resulting bytes. A small hand-rolled top-level-key scanner
//! (`extract_keys`, below) is test-only fixture/assertion code, not a JSON codec re-implementation
//! — it exists solely to assert on `condense`'s own output shape.
//!
//! Scope: field-priority / budget behavior of `lib_net::mesh::condense` only. Radio serial I/O
//! (`lib_net::mesh::run_radio`) and the hand-rolled Meshtastic protobuf + stream framing
//! (`lib_net::mesh::MeshFrame`/`ToRadioFrame`) are NOT exercised here — see
//! `crates/lib-mesh/src/impl_frame.rs`'s inline byte-level golden tests for the latter, which need
//! no broker/serial harness and so stay in `lib-mesh`'s own `cargo test`.
//!
//! PROVISIONAL: every scenario in `meshtastic_condense.feature` is tagged `@provisional` — see
//! that file's header and `crates/lib-mesh/src/impl_condense.rs`'s module doc comment for why (no
//! reference `meteor-adapter` implementation was available in this environment).
//!
//! Test code only; freely uses unwrap/expect/panic (the workspace lint gate denies these in
//! production logic, not tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cucumber::{given, then, when};

use lib_net::locate::{LocationFix, LocationSource};
use lib_net::mesh::CondenseInput;
use lib_parse::canonical::{CanonicalFault, CanonicalReading};

use crate::SpecWorld;

/// Scenario scratch for the Meshtastic mesh-radio feature area. cucumber resets `SpecWorld` per
/// scenario, so every field starts `None`/empty and is populated only by the steps a given
/// scenario actually runs.
#[derive(Debug, Default)]
pub struct MeshState {
    reading: Option<CanonicalReading>,
    loc: Option<LocationFix>,
    /// An active pack fault/warning set to thread through `CondenseInput::fault` (#387 — mesh
    /// fault forwarding). `None` unless a scenario stages one.
    fault: Option<CanonicalFault>,
    /// The bytes `lib_net::mesh::condense` wrote, populated by the `When` step.
    condensed: Vec<u8>,
}

/// A representative battery reading: nonzero SOC/voltage/current, a few cells (with a deliberate
/// min/max spread to exercise cell-imbalance reporting), and two temperature sensors. Not tied to
/// any specific Seplos capture — see this file's module doc comment for the PROVISIONAL caveat on
/// `condense`'s field shape overall.
fn typical_reading() -> CanonicalReading {
    let mut cells = heapless::Vec::new();
    cells.push(3800u16).expect("cell fits MAX_CELLS");
    cells.push(3850u16).expect("cell fits MAX_CELLS");
    cells.push(3820u16).expect("cell fits MAX_CELLS");
    let mut temps = heapless::Vec::new();
    temps.push(2981u16).expect("temp fits MAX_TEMPS");
    temps.push(2990u16).expect("temp fits MAX_TEMPS");
    CanonicalReading {
        device_address: 3,
        cell_voltages_mv: cells,
        temperatures_raw: temps,
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

/// Scan a `condense`-produced compact JSON object for its top-level keys, in the order they
/// appear. Tracks `[`/`]` depth so the single nested array field (`"t":[...]`) doesn't confuse the
/// scan. Test-only fixture/assertion helper, not a general JSON parser.
fn extract_keys(json: &str) -> Vec<String> {
    let bytes = json.as_bytes();
    let mut keys = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }
        let start = i + 1;
        let mut j = start;
        while j < bytes.len() && bytes[j] != b'"' {
            j += 1;
        }
        keys.push(json[start..j].to_string());
        i = j + 1;
        while i < bytes.len() && bytes[i] != b':' {
            i += 1;
        }
        i += 1; // skip ':'
        let mut depth = 0i32;
        while i < bytes.len() {
            match bytes[i] {
                b'[' => depth += 1,
                b']' => depth -= 1,
                b',' | b'}' if depth == 0 => break,
                _ => {}
            }
            i += 1;
        }
    }
    keys
}

// ---- Given: fixtures -----------------------------------------------------------------------

#[given("a typical battery reading")]
fn given_typical_reading(world: &mut SpecWorld) {
    world.mesh.reading = Some(typical_reading());
}

#[given("a battery reading with no cell or temperature data")]
fn given_empty_reading(world: &mut SpecWorld) {
    let mut r = typical_reading();
    r.cell_voltages_mv = heapless::Vec::new();
    r.temperatures_raw = heapless::Vec::new();
    world.mesh.reading = Some(r);
}

#[given(expr = "a location fix at lat {float} lon {float}")]
fn given_location_fix(world: &mut SpecWorld, lat: f64, lon: f64) {
    world.mesh.loc = Some(LocationFix {
        lat,
        lon,
        alt_m: 0.0,
        source: LocationSource::GpsNmea,
        ts_ms: 0,
    });
}

/// Stage an active pack fault (#387 — mesh fault forwarding): a single tripped flag
/// (`high_voltage_protection`, bit 0) is enough to exercise `CondenseInput::fault` and the
/// resulting `"f"` bitmask field without pinning the scenario to every flag's exact bit position.
#[given("an active high-voltage protection fault")]
fn given_active_fault(world: &mut SpecWorld) {
    world.mesh.fault = Some(CanonicalFault {
        high_voltage_protection: true,
        ..CanonicalFault::default()
    });
}

// ---- When: the real lib_net::mesh::condense call ------------------------------------------------

#[when(expr = "the reading is condensed for mesh transmission with a budget of {int} byte")]
#[when(expr = "the reading is condensed for mesh transmission with a budget of {int} bytes")]
fn when_condense(world: &mut SpecWorld, max: i64) {
    let reading = world.mesh.reading.clone().expect("a reading was staged");
    let max = max as usize;
    let input = CondenseInput {
        reading: &reading,
        fault: world.mesh.fault.as_ref(),
        loc: world.mesh.loc.as_ref(),
    };
    // `condense` is the no_std/no-alloc buffer-based codec (#387/#386 rewrite): it writes directly
    // into a caller-supplied `&mut [u8]` rather than a growable `Vec`, so the scratch buffer must
    // already be sized to (at least) the requested budget — an empty `Vec` would coerce to a
    // zero-length slice and `condense` would always write nothing.
    let mut out = vec![0u8; max];
    let n = lib_net::mesh::condense(&input, max, &mut out);
    out.truncate(n);
    world.mesh.condensed = out;
}

// ---- Then: assertions ----------------------------------------------------------------------

#[then(expr = "the condensed payload is valid JSON at most {int} bytes")]
fn then_valid_json_within_budget(world: &mut SpecWorld, max: i64) {
    let s = std::str::from_utf8(&world.mesh.condensed).expect("condense emits valid UTF-8");
    assert!(
        s.len() <= max as usize,
        "{} exceeds the {max}-byte budget",
        s.len()
    );
    assert!(
        s.starts_with('{') && s.ends_with('}'),
        "expected a JSON object, got {s}"
    );
}

#[then(expr = "the condensed payload keys are, in order, {string}")]
fn then_keys_in_order(world: &mut SpecWorld, expected_csv: String) {
    let s = std::str::from_utf8(&world.mesh.condensed).expect("condense emits valid UTF-8");
    // An empty Examples cell means "no keys at all" (`{}`) — `"".split(',')` would otherwise
    // yield a single empty-string element instead of an empty list.
    let expected: Vec<String> = if expected_csv.is_empty() {
        Vec::new()
    } else {
        expected_csv.split(',').map(str::to_string).collect()
    };
    assert_eq!(extract_keys(s), expected, "in {s}");
}

#[then("the condensed payload is empty")]
fn then_condensed_is_empty(world: &mut SpecWorld) {
    assert!(
        world.mesh.condensed.is_empty(),
        "expected no bytes written, got {:?}",
        world.mesh.condensed
    );
}

#[then(expr = "the condensed payload contains soc {int}")]
fn then_contains_soc(world: &mut SpecWorld, soc: i64) {
    let s = std::str::from_utf8(&world.mesh.condensed).expect("condense emits valid UTF-8");
    assert!(
        s.contains(&format!("\"soc\":{soc}")),
        "expected soc={soc} in {s}"
    );
}

/// #387 — asserts the compact fault bitmask field's exact value (see
/// `lib_net::mesh::impl_condense::fault_bitmask`'s doc comment for the bit order).
#[then(expr = "the condensed payload contains fault bitmask {int}")]
fn then_contains_fault_bitmask(world: &mut SpecWorld, mask: i64) {
    let s = std::str::from_utf8(&world.mesh.condensed).expect("condense emits valid UTF-8");
    assert!(
        s.contains(&format!("\"f\":{mask}")),
        "expected fault bitmask {mask} in {s}"
    );
}
