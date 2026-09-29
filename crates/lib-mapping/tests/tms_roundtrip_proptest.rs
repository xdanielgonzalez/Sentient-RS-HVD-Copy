//! Property tests: canonical → TMS → JSON → TMS round-trip, via the real production path
//! (`transform::{ToTms, ToDiagnostics}` → `codec::to_json_slice` → `serde_json_core::from_slice`).
//!
//! `EnergyStorageState::run_time` and `PowerPortMeasurement::power_rate_of_change` are *always*
//! `f32::NAN` (hardcoded in `transform.rs` — no upstream source yet), so a naive `assert_eq!` on
//! the decoded struct is wrong: IEEE-754 `NaN != NaN`, even bit-identical NaNs. The documented
//! convention (`codec.rs`) is `NaN` serializes to JSON `null`; `serde-json-core` deserializes
//! `null` back to `NaN` (confirmed empirically). So "round-trip is identity" is proven at the
//! **JSON-bytes level**: serialize once, decode, serialize the decoded value again, and the two
//! JSON byte strings must be identical. That sidesteps `NaN` `PartialEq` entirely while still proving
//! the codec is loss-free on every field the standard can carry.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use heapless::{String as HString, Vec as HVec};
use lib_mapping::codec::to_json_slice;
use lib_mapping::schema::{
    ActiveDiagnosticState, ClockMonotonic, DcMeasurementUpdate, PowerHardwareUpdate,
};
use lib_mapping::transform::{FromTms, ToDiagnostics, ToTms};
use lib_parse::canonical::{CanonicalFault, CanonicalReading, MAX_CELLS, MAX_TEMPS};
use proptest::prelude::*;

const BUF_LEN: usize = 8192;

/// Serialize `value`, deserialize it back into `T`, re-serialize, and return both JSON byte
/// buffers (as owned `Vec<u8>`, so the caller can compare/print without lifetime juggling).
fn json_roundtrip_bytes<T>(value: &T) -> (Vec<u8>, Vec<u8>)
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let mut buf1 = [0u8; BUF_LEN];
    let n1 = to_json_slice(value, &mut buf1).expect("serialize");

    let (decoded, used): (T, usize) =
        serde_json_core::from_slice(&buf1[..n1]).expect("deserialize");
    assert_eq!(used, n1, "deserializer must consume the whole payload");

    let mut buf2 = [0u8; BUF_LEN];
    let n2 = to_json_slice(&decoded, &mut buf2).expect("re-serialize");

    (buf1[..n1].to_vec(), buf2[..n2].to_vec())
}

fn device_id(tag: u8) -> HString<32> {
    // Keep it short + always-valid ASCII so it never overflows the bounded String<32>.
    let mut s = HString::new();
    let _ = s.push_str("dev-");
    let _ = s.push((b'0' + tag / 100) as char);
    let _ = s.push((b'0' + (tag / 10) % 10) as char);
    let _ = s.push((b'0' + tag % 10) as char);
    s
}

fn bounded_vec<T, const N: usize>(items: impl IntoIterator<Item = T>) -> HVec<T, N> {
    let mut v = HVec::new();
    for item in items {
        let _ = v.push(item);
    }
    v
}

proptest! {
    /// `DcMeasurementUpdate` (contains the always-NaN `power_rate_of_change`) round-trips through
    /// JSON as an identity at the byte level, for any cell/temp count in the valid bounded range
    /// and any raw field values.
    #[test]
    fn dc_measurement_json_round_trip_is_identity(
        device_tag in any::<u8>(),
        seconds in any::<u32>(),
        nanoseconds in any::<u32>(),
        cells in proptest::collection::vec(any::<u16>(), 0..=MAX_CELLS),
        temps in proptest::collection::vec(any::<u16>(), 0..=MAX_TEMPS),
        current_ca in any::<i16>(),
        total_voltage_cv in any::<u16>(),
        remaining_capacity_cah in any::<u16>(),
        typical_capacity_cah in any::<u16>(),
        soc_dpct in any::<u16>(),
        rated_capacity_cah in any::<u16>(),
        cycles in any::<u16>(),
        soh_dpct in any::<u16>(),
        port_voltage_cv in any::<u16>(),
    ) {
        let reading = CanonicalReading {
            device_address: 0,
            cell_voltages_mv: bounded_vec(cells),
            temperatures_raw: bounded_vec(temps),
            current_ca,
            total_voltage_cv,
            remaining_capacity_cah,
            typical_capacity_cah,
            rated_capacity_cah,
            soc_dpct,
            soh_dpct,
            cycles,
            port_voltage_cv,
        };
        let now = ClockMonotonic { seconds, nanoseconds };
        let update: DcMeasurementUpdate = reading.to_dc_measurement(device_id(device_tag), now);

        let (first, second) = json_roundtrip_bytes(&update);
        prop_assert_eq!(first, second);
    }

    /// `PowerHardwareUpdate` (carries `EnergyStorageState`, whose `run_time` is always NaN, and
    /// whose min/max/average temperature are NaN precisely when `temps` is empty) round-trips.
    #[test]
    fn power_hardware_json_round_trip_is_identity(
        device_tag in any::<u8>(),
        seconds in any::<u32>(),
        cells in proptest::collection::vec(any::<u16>(), 0..=MAX_CELLS),
        temps in proptest::collection::vec(any::<u16>(), 0..=MAX_TEMPS),
        current_ca in any::<i16>(),
        total_voltage_cv in any::<u16>(),
        soc_dpct in any::<u16>(),
        soh_dpct in any::<u16>(),
        cycles in any::<u16>(),
    ) {
        let reading = CanonicalReading {
            device_address: 0,
            cell_voltages_mv: bounded_vec(cells),
            temperatures_raw: bounded_vec(temps),
            current_ca,
            total_voltage_cv,
            remaining_capacity_cah: 0,
            typical_capacity_cah: 0,
            rated_capacity_cah: 0,
            soc_dpct,
            soh_dpct,
            cycles,
            port_voltage_cv: 0,
        };
        let now = ClockMonotonic { seconds, nanoseconds: 0 };
        let update: PowerHardwareUpdate = reading.to_power_hardware(device_id(device_tag), now);

        let (first, second) = json_roundtrip_bytes(&update);
        prop_assert_eq!(first, second);
    }

    /// `ActiveDiagnosticState` (no floats — every flag combination) round-trips exactly.
    #[test]
    fn diagnostics_json_round_trip_is_identity(
        device_tag in any::<u8>(),
        flags in proptest::collection::vec(any::<bool>(), 16),
    ) {
        let fault = CanonicalFault {
            device_address: 0,
            high_voltage_protection: flags[0],
            low_voltage_protection: flags[1],
            high_temperature_charge_protection: flags[2],
            low_temperature_charge_protection: flags[3],
            charge_overcurrent_protection: flags[4],
            discharge_overcurrent_protection: flags[5],
            short_circuit_protection: flags[6],
            mosfet_fault: flags[7],
            cell_balance_warning: flags[8],
            low_capacity_warning: flags[9],
            high_temperature_discharge_warning: flags[10],
            low_temperature_discharge_warning: flags[11],
            low_voltage_warning: flags[12],
            high_voltage_warning: flags[13],
            discharge_current_warning: flags[14],
            charge_current_warning: flags[15],
        };
        let now = ClockMonotonic { seconds: 0, nanoseconds: 0 };
        let diag: ActiveDiagnosticState = fault.to_active_diagnostics(device_id(device_tag), now);

        let (first, second) = json_roundtrip_bytes(&diag);
        prop_assert_eq!(first, second);
    }

/// `CanonicalReading::from_tms` recovers the fields documented as exact in `FromTms`, for any
    /// cell/temp count where cells >= temps (the documented exact-reconstruction case).
    #[test]
    fn from_tms_recovers_documented_exact_fields(
        device_tag in any::<u8>(),
        cells in proptest::collection::vec(any::<u16>(), 1..=MAX_CELLS),
        current_ca in any::<i16>(),
        total_voltage_cv in any::<u16>(),
        soc_dpct in any::<u16>(),
        cycles in any::<u16>(),
    ) {
        // At most as many temps as cells, so this hits FromTms's documented exact case.
        let temp_count = cells.len().min(MAX_TEMPS);
        let temps: Vec<u16> = (0..temp_count).map(|i| 2731u16.wrapping_add(i as u16 * 7)).collect();

        let reading = CanonicalReading {
            device_address: 0,
            cell_voltages_mv: bounded_vec(cells),
            temperatures_raw: bounded_vec(temps),
            current_ca,
            total_voltage_cv,
            remaining_capacity_cah: 0,
            typical_capacity_cah: 0,
            rated_capacity_cah: 0,
            soc_dpct,
            soh_dpct: 0,
            cycles,
            port_voltage_cv: 0,
        };
        let now = ClockMonotonic { seconds: 0, nanoseconds: 0 };
        let dc = reading.to_dc_measurement(device_id(device_tag), now);
        let hw = reading.to_power_hardware(device_id(device_tag), now);

        let decoded = CanonicalReading::from_tms(&dc, &hw);
        prop_assert_eq!(decoded.total_voltage_cv, reading.total_voltage_cv);
        prop_assert_eq!(decoded.current_ca, reading.current_ca);
        prop_assert_eq!(decoded.cell_voltages_mv, reading.cell_voltages_mv);
        prop_assert_eq!(decoded.soc_dpct, reading.soc_dpct);
        prop_assert_eq!(decoded.cycles, reading.cycles);
        prop_assert_eq!(decoded.temperatures_raw, reading.temperatures_raw);
    }
}
