//! no_std JSON codec (serde-json-core) round-trip-ish checks.

// Test code may unwrap/panic; the workspace denies these in logic only.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use heapless::{String as HString, Vec as HVec};
use lib_mapping::codec::to_json_slice;
use lib_mapping::schema::ClockMonotonic;
use lib_mapping::transform::ToTms;
use lib_parse::canonical::CanonicalReading;

fn dev() -> HString<32> {
    let mut s = HString::new();
    s.push_str("METEOR-001").ok();
    s
}

fn sample() -> CanonicalReading {
    let mut cells = HVec::new();
    cells.push(3894).ok();
    CanonicalReading {
        device_address: 0,
        cell_voltages_mv: cells,
        temperatures_raw: HVec::new(),
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

#[test]
fn dc_measurement_serializes_to_json_slice() {
    let now = ClockMonotonic {
        seconds: 1000,
        nanoseconds: 0,
    };
    let update = sample().to_dc_measurement(dev(), now);
    let mut buf = [0u8; 1024];
    let n = to_json_slice(&update, &mut buf).expect("serialize");
    let s = core::str::from_utf8(&buf[..n]).expect("utf8");
    assert!(s.contains("\"voltage\":54.51"), "got: {s}");
    assert!(s.contains("METEOR-001"));
}
