//! Spot-check the Seplos → TMS mapping against a real device capture (the same `LIVE_FRAME` used
//! by `lib-parse`'s `seplos_vectors.rs`), through BOTH wire codecs.
//!
//! `LIVE_FRAME` is a genuine 14-cell telemetry response (Seplos 48150-1101-10E, DIP 0). Known raw
//! values (cross-checked in `lib-parse/tests/seplos_vectors.rs::decodes_live_telemetry`):
//! total_voltage_cv=5452 (54.52 V), current_ca=-164 (discharging 1.64 A), soc_dpct=775 (77.5%),
//! soh_dpct=1000 (100%), cycles=2, 14 cells (cell[0]=3894 mV, min=3893 mV, max=3895 mV).

// `lib-parse`'s `seplos` feature is pulled in via this crate's `[dev-dependencies]` override
// (see `Cargo.toml`), so no `#[cfg(feature = ...)]` gate is needed here.
// Test code may unwrap/panic; the workspace denies these in logic only.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use heapless::String;
use lib_mapping::codec::to_json_slice;
use lib_mapping::schema::{ClockMonotonic, DcMeasurementUpdate, PowerHardwareUpdate};
use lib_mapping::transform::{FromTms, ToTms};
use lib_parse::seplos::Seplos;
use lib_parse::vendor::BmsVendorProtocol;

const LIVE_FRAME: &[u8] = b"~20004600A08E00000E0F360F360F360F360F350F360F360F360F360F370F370F360F360F35060AF10AF90AF90AF30B370B0CFF5C154C36820A465003074650000203E8154CFFF800000079004DDEC8\r";

fn dev() -> String<32> {
    let mut s = String::new();
    s.push_str("METEOR-001").ok();
    s
}

fn now() -> ClockMonotonic {
    ClockMonotonic {
        seconds: 42,
        nanoseconds: 7,
    }
}

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.005
}

#[test]
fn live_frame_write_matches_known_tms_values() {
    let reading = Seplos::new()
        .parse_reading(LIVE_FRAME, 0)
        .expect("decode LIVE_FRAME");

    let dc = reading.to_dc_measurement(dev(), now());
    let line = &dc.external_measurement[0].line[0];
    assert!(approx(line.voltage, 54.52), "voltage: {}", line.voltage);
    // Vendor discharging (negative current) -> TMS positive (sourcing into the microgrid).
    assert!(approx(line.amperage, 1.64), "amperage: {}", line.amperage);

    let hw = reading.to_power_hardware(dev(), now());
    let es = hw.energy_storage.expect("energy_storage present");
    assert_eq!(es.storage_cells.len(), 14);
    assert!(approx(es.storage_cells[0].voltage, 3.894));
    // #278: `state_of_charge` is `Option<f32>` (IDL `@optional`); the mapper always sets `Some(_)`.
    assert!(approx(es.storage_cells[0].state_of_charge.unwrap(), 0.775));
    assert!(approx(es.storage_cells[0].cycle_counter, 2.0));
    let min_mv = es
        .storage_cells
        .iter()
        .map(|c| (c.voltage * 1000.0).round() as u32)
        .min()
        .unwrap();
    let max_mv = es
        .storage_cells
        .iter()
        .map(|c| (c.voltage * 1000.0).round() as u32)
        .max()
        .unwrap();
    assert_eq!(min_mv, 3893);
    assert_eq!(max_mv, 3895);
}

#[test]
fn live_frame_read_after_write_round_trips_through_json() {
    let reading = Seplos::new()
        .parse_reading(LIVE_FRAME, 0)
        .expect("decode LIVE_FRAME");

    let dc = reading.to_dc_measurement(dev(), now());
    let hw = reading.to_power_hardware(dev(), now());

    let mut dc_buf = [0u8; 4096];
    let dc_n = to_json_slice(&dc, &mut dc_buf).expect("serialize dc");
    let (dc_decoded, _): (DcMeasurementUpdate, usize) =
        serde_json_core::from_slice(&dc_buf[..dc_n]).expect("deserialize dc");

    let mut hw_buf = [0u8; 4096];
    let hw_n = to_json_slice(&hw, &mut hw_buf).expect("serialize hw");
    let (hw_decoded, _): (PowerHardwareUpdate, usize) =
        serde_json_core::from_slice(&hw_buf[..hw_n]).expect("deserialize hw");

    let decoded = lib_parse::canonical::CanonicalReading::from_tms(&dc_decoded, &hw_decoded);
    assert_eq!(decoded.total_voltage_cv, reading.total_voltage_cv);
    assert_eq!(decoded.current_ca, reading.current_ca);
    assert_eq!(decoded.cell_voltages_mv, reading.cell_voltages_mv);
    assert_eq!(decoded.soc_dpct, reading.soc_dpct);
    assert_eq!(decoded.cycles, reading.cycles);
}
