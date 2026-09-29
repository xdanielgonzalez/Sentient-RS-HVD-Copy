//! AWS-Greengrass backwards-compat merged flat legacy schema (Scope D, closes #52, reconciled
//! against the real baseline — gap-closure #269) — see `docs/gap-analysis-meteor-parity.md`'s
//! "Greengrass local IPC publish" finding. Feature `legacy` (requires `std`, not part of
//! `default`) — a device that never bridges to the legacy AWS Greengrass pipeline doesn't compile
//! this in at all.
//!
//! **Baseline-VERIFIED, not provisional.** The reference prototype is checked in and readable at
//! `/mnt/d/hvd/meteor-adapter` (branch `master`) — every field name, scale factor, and omission
//! below was checked directly against
//! `example/aws/src/greengrass/telemetry/battery/battery_reader.py`,
//! `example/aws/src/greengrass/telemetry/battery/config.yaml`, and
//! `example/aws/src/greengrass/telemetry/common/device_telemetry_base.py` (the base
//! timestamp/device/location fields every device type shares). The baseline publishes ONE merged
//! flat JSON object per battery per poll cycle — the current reading AND the most recently cached
//! fault flags in the SAME message ([`crate::legacy::battery_payload`] builds exactly that). There is no longer a
//! separate typed `LegacyReading`/`LegacyFault`/`LegacyLocation` triad — that shape was a
//! provisional best-effort guess made before the baseline was available in this environment; see
//! git history for `impl_legacy_reading.rs` et al. if the old (superseded) shape is ever needed.
//!
//! [`crate::legacy::LegacyLocationFix`] is the ONLY type this module defines — a location tag in the module's own
//! vocabulary (never `lib_locate::LocationFix`; `lib-mapping` must not depend on `lib-locate` —
//! see that type's own doc comment for the full rationale). Rendering goes straight to
//! `serde_json::Value`/`Map` — plain `serde_json`, never the no_std `sink`/`codec`/`wire` TMS path
//! (this module has its own quarantined feature gate; see `Cargo.toml`'s `legacy` feature
//! comment). **Quarantine: Scope-D-only — this module must never be reached from any TMS publish
//! path** (the `tms/#` topics stay pure `lib_mapping::render` output); the only producer is
//! `impl_app::host::GreengrassTap`, publishing to `telemetry/{device_type}/{device_id}`.
//!
//! The Voltronic/MPP-Solar inverter side of Scope D (`telemetry/inverter/{device_id}`) lives in the
//! sibling `crate::legacy::inverter` submodule ([`crate::legacy::inverter_payload`]) — added once the canonical inverter-reading
//! type landed (Wave 2) and wired end to end in Wave 3 (D10b). Unlike `battery_payload` (baseline-
//! VERIFIED), the inverter payload is PROVISIONAL (no real Voltronic captures — decision 0001).

mod inverter;
mod types;

pub use inverter::inverter_payload;
pub use types::*;

use serde_json::{Map, Value};

use lib_parse::canonical::{CanonicalFault, CanonicalReading};

/// Format `ms_since_epoch` (Unix epoch milliseconds, UTC) as an ISO-8601 string with millisecond
/// precision and a trailing `Z`, e.g. `"2026-07-16T12:34:56.789Z"` — matches the baseline's
/// `datetime.utcnow().isoformat() + 'Z'` (`device_telemetry_base.py`, `collect_telemetry`).
/// Hand-rolled: no `chrono`/`time` dependency (this crate stays lean even under `std`), using
/// Howard Hinnant's `civil_from_days` algorithm
/// (<http://howardhinnant.github.io/date_algorithms.html>) to turn a day count into a
/// proleptic-Gregorian `(year, month, day)`.
pub fn iso8601_utc_ms(ms_since_epoch: u64) -> String {
    let millis = ms_since_epoch % 1000;
    let total_secs = ms_since_epoch / 1000;
    let secs_of_day = total_secs % 86_400;
    let days = (total_secs / 86_400) as i64;

    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;

    let (year, month, day) = civil_from_days(days);

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

/// Howard Hinnant's `civil_from_days`: a day count since 1970-01-01 -> `(year, month, day)` in the
/// proleptic Gregorian calendar. The algorithm itself handles negative `z` (pre-epoch days) even
/// though this module's only caller ([`iso8601_utc_ms`], fed by a `u64` millisecond count) never
/// passes one — kept in its general (signed) form since that's how the reference derivation is
/// stated and verified.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// `(flag name, is set)` pairs for every `CanonicalFault` flag, in the struct's declared order —
/// which is also the baseline's `status_command.fault_bits` order (`config.yaml`). The single
/// source of truth [`crate::legacy::battery_payload`] walks to merge the 0/1 fault-flag keys into the payload.
fn fault_flags(fault: &CanonicalFault) -> [(&'static str, bool); 16] {
    [
        ("high_voltage_protection", fault.high_voltage_protection),
        ("low_voltage_protection", fault.low_voltage_protection),
        (
            "high_temperature_charge_protection",
            fault.high_temperature_charge_protection,
        ),
        (
            "low_temperature_charge_protection",
            fault.low_temperature_charge_protection,
        ),
        (
            "charge_overcurrent_protection",
            fault.charge_overcurrent_protection,
        ),
        (
            "discharge_overcurrent_protection",
            fault.discharge_overcurrent_protection,
        ),
        ("short_circuit_protection", fault.short_circuit_protection),
        ("mosfet_fault", fault.mosfet_fault),
        ("cell_balance_warning", fault.cell_balance_warning),
        ("low_capacity_warning", fault.low_capacity_warning),
        (
            "high_temperature_discharge_warning",
            fault.high_temperature_discharge_warning,
        ),
        (
            "low_temperature_discharge_warning",
            fault.low_temperature_discharge_warning,
        ),
        ("low_voltage_warning", fault.low_voltage_warning),
        ("high_voltage_warning", fault.high_voltage_warning),
        ("discharge_current_warning", fault.discharge_current_warning),
        ("charge_current_warning", fault.charge_current_warning),
    ]
}

/// Build ONE merged flat battery telemetry JSON payload — baseline-verified against
/// `example/aws/src/greengrass/telemetry/battery/{battery_reader.py,config.yaml}` and
/// `common/device_telemetry_base.py` (see this module's doc comment). `device_id` is the caller's
/// unit-level identity (the baseline's `self.device_id`, NOT a per-frame Seplos-address
/// derivation — see `impl_app::host::GreengrassTap`, the only real caller). `fault` is `None`
/// until a fault poll has succeeded at least once (cache semantics owned by the caller — see
/// `impl_greengrass_tap.rs`'s `WAVE-D1` marker for the current state of that cache); while
/// `None`, none of the 16 fault-flag keys are emitted at all (not even as `0`), matching the
/// baseline's own merge-on-success behavior.
///
/// Field set (baseline `config.yaml` `enabled: true` fields only):
/// - base: `timestamp` (ISO-8601 Z), `device_id`, `device_type` = `"battery"`, flat `latitude`/
///   `longitude` (KEYS OMITTED ENTIRELY when `loc` is `None` — not emitted as JSON `null`,
///   matching the baseline exactly), `location_source` (`"unknown"` when `loc` is `None`),
///   `location_timestamp` (the fix's own `ts_ms`, ISO-8601 Z, when present, else `wall_ms`)
/// - `num_cells`, flat `cell_{i}_voltage` (mV ints, 0-based), flat `temperature_{j}` (°C floats,
///   1-based, capped at 6 sensors — the baseline's `config.yaml` only enables 6)
/// - `current` (A), `residual_capacity` (Ah), `battery_capacity` (Ah, from the reading's *typical*
///   capacity field — the baseline's `battery_capacity` wire field is a distinct byte range from
///   its `rated_capacity` one, and the latter is `enabled: false`), `state_of_charge` (%),
///   `state_of_health` (%), `num_cycles`, `port_voltage` (V), `dip_address`
/// - the 16 fault flags as 0/1 ints, only when `fault.is_some()`
///
/// OMITTED entirely (baseline `config.yaml`: `enabled: false`): `total_voltage`, `rated_capacity`,
/// `custom_number`.
pub fn battery_payload(
    device_id: &str,
    r: &CanonicalReading,
    fault: Option<&CanonicalFault>,
    loc: Option<&LegacyLocationFix>,
    wall_ms: u64,
    dip_address: u8,
) -> serde_json::Result<Vec<u8>> {
    let mut m = Map::new();

    m.insert(
        "timestamp".to_string(),
        Value::String(iso8601_utc_ms(wall_ms)),
    );
    m.insert(
        "device_id".to_string(),
        Value::String(device_id.to_string()),
    );
    m.insert(
        "device_type".to_string(),
        Value::String("battery".to_string()),
    );

    match loc {
        Some(fix) => {
            m.insert("latitude".to_string(), Value::from(fix.latitude));
            m.insert("longitude".to_string(), Value::from(fix.longitude));
            m.insert(
                "location_source".to_string(),
                Value::String(fix.source.to_string()),
            );
            m.insert(
                "location_timestamp".to_string(),
                Value::String(iso8601_utc_ms(fix.ts_ms)),
            );
        }
        None => {
            // Baseline (`device_telemetry_base.collect_telemetry`) omits the `latitude`/
            // `longitude` keys entirely when no fix is available — it never emits them as JSON
            // `null`. `location_source`/`location_timestamp` are still emitted.
            m.insert(
                "location_source".to_string(),
                Value::String("unknown".to_string()),
            );
            m.insert(
                "location_timestamp".to_string(),
                Value::String(iso8601_utc_ms(wall_ms)),
            );
        }
    }

    m.insert(
        "num_cells".to_string(),
        Value::from(r.cell_voltages_mv.len() as u64),
    );
    for (i, mv) in r.cell_voltages_mv.iter().enumerate() {
        m.insert(format!("cell_{i}_voltage"), Value::from(*mv));
    }
    for (j, raw) in r.temperatures_raw.iter().enumerate().take(6) {
        let celsius = (*raw as f64 - 2731.0) / 10.0;
        m.insert(format!("temperature_{}", j + 1), Value::from(celsius));
    }

    m.insert(
        "current".to_string(),
        Value::from(r.current_ca as f64 * 0.01),
    );
    m.insert(
        "residual_capacity".to_string(),
        Value::from(r.remaining_capacity_cah as f64 * 0.01),
    );
    m.insert(
        "battery_capacity".to_string(),
        Value::from(r.typical_capacity_cah as f64 * 0.01),
    );
    m.insert(
        "state_of_charge".to_string(),
        Value::from(r.soc_dpct as f64 * 0.1),
    );
    m.insert(
        "state_of_health".to_string(),
        Value::from(r.soh_dpct as f64 * 0.1),
    );
    m.insert("num_cycles".to_string(), Value::from(r.cycles));
    m.insert(
        "port_voltage".to_string(),
        Value::from(r.port_voltage_cv as f64 * 0.01),
    );
    m.insert("dip_address".to_string(), Value::from(dip_address));

    if let Some(f) = fault {
        for (name, set) in fault_flags(f) {
            m.insert(name.to_string(), Value::from(u8::from(set)));
        }
    }

    serde_json::to_vec(&Value::Object(m))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    // ---- iso8601_utc_ms -----------------------------------------------------------------------

    #[test]
    fn epoch_zero_is_the_unix_epoch() {
        assert_eq!(iso8601_utc_ms(0), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn leap_day_2024_02_29() {
        // `date -u -d '2024-02-29T00:00:00Z' +%s` == 1_709_164_800
        assert_eq!(
            iso8601_utc_ms(1_709_164_800_000),
            "2024-02-29T00:00:00.000Z"
        );
    }

    #[test]
    fn a_2026_07_16_sample_with_millis() {
        // `date -u -d '2026-07-16T12:34:56Z' +%s` == 1_784_205_296
        assert_eq!(
            iso8601_utc_ms(1_784_205_296_789),
            "2026-07-16T12:34:56.789Z"
        );
    }

    #[test]
    fn year_rollover() {
        // `date -u -d '2025-12-31T23:59:59Z' +%s` == 1_767_225_599
        assert_eq!(
            iso8601_utc_ms(1_767_225_599_999),
            "2025-12-31T23:59:59.999Z"
        );
        assert_eq!(
            iso8601_utc_ms(1_767_225_600_000),
            "2026-01-01T00:00:00.000Z"
        );
    }

    // ---- battery_payload -----------------------------------------------------------------------

    fn sample_reading() -> CanonicalReading {
        let mut cells = heapless::Vec::new();
        cells.push(3894).unwrap();
        cells.push(3901).unwrap();
        let mut temps = heapless::Vec::new();
        temps.push(2991u16).unwrap(); // (2991 - 2731) / 10 = 26.0 C
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

    fn parse(bytes: &[u8]) -> Value {
        serde_json::from_slice(bytes).expect("valid json")
    }

    #[test]
    fn golden_battery_json_without_fault_or_location() {
        let reading = sample_reading();
        let bytes = battery_payload("METEOR-001", &reading, None, None, 1_700_000_000_000, 3)
            .expect("serialize");
        let v = parse(&bytes);

        assert_eq!(v["device_id"], "METEOR-001");
        assert_eq!(v["device_type"], "battery");
        assert!(v["timestamp"].as_str().unwrap().ends_with('Z'));
        // Baseline omits `latitude`/`longitude` entirely when no fix is available — indexing a
        // missing key on a JSON object also yields `Value::Null`, so this assertion additionally
        // confirms the keys are ABSENT, not merely null-valued.
        assert!(!v.as_object().unwrap().contains_key("latitude"));
        assert!(!v.as_object().unwrap().contains_key("longitude"));
        assert!(v["latitude"].is_null());
        assert!(v["longitude"].is_null());
        assert_eq!(v["location_source"], "unknown");
        assert!(v["location_timestamp"].as_str().unwrap().ends_with('Z'));

        assert_eq!(v["num_cells"], 2);
        assert_eq!(v["cell_0_voltage"], 3894);
        assert_eq!(v["cell_1_voltage"], 3901);
        assert!((v["temperature_1"].as_f64().unwrap() - 26.0).abs() < 1e-9);

        assert!((v["current"].as_f64().unwrap() - (-1.44)).abs() < 1e-9);
        assert!((v["residual_capacity"].as_f64().unwrap() - 139.53).abs() < 1e-9);
        assert!((v["battery_capacity"].as_f64().unwrap() - 180.0).abs() < 1e-9);
        assert!((v["state_of_charge"].as_f64().unwrap() - 77.5).abs() < 1e-9);
        assert!((v["state_of_health"].as_f64().unwrap() - 100.0).abs() < 1e-9);
        assert_eq!(v["num_cycles"], 2);
        assert!((v["port_voltage"].as_f64().unwrap() - 54.53).abs() < 1e-9);
        assert_eq!(v["dip_address"], 3);

        // Baseline-disabled fields never appear.
        assert!(v.get("total_voltage").is_none());
        assert!(v.get("rated_capacity").is_none());
        assert!(v.get("custom_number").is_none());

        // No fault polled yet -> no fault-flag keys at all (not even `0`).
        assert!(v.get("high_voltage_protection").is_none());
        assert!(v.get("mosfet_fault").is_none());
    }

    #[test]
    fn golden_battery_json_with_location() {
        let reading = sample_reading();
        let loc = LegacyLocationFix {
            latitude: 37.7749,
            longitude: -122.4194,
            source: "gps_nmea",
            ts_ms: 1_700_000_000_500,
        };
        let bytes = battery_payload(
            "METEOR-001",
            &reading,
            None,
            Some(&loc),
            1_700_000_000_000,
            3,
        )
        .expect("serialize");
        let v = parse(&bytes);

        assert!((v["latitude"].as_f64().unwrap() - 37.7749).abs() < 1e-9);
        assert!((v["longitude"].as_f64().unwrap() - (-122.4194)).abs() < 1e-9);
        assert_eq!(v["location_source"], "gps_nmea");
        assert_eq!(v["location_timestamp"], iso8601_utc_ms(1_700_000_000_500));
    }

    #[test]
    fn golden_battery_json_with_fault_merges_all_16_flags() {
        let reading = sample_reading();
        let fault = CanonicalFault {
            device_address: 3,
            high_voltage_protection: true,
            mosfet_fault: true,
            ..Default::default()
        };
        let bytes = battery_payload(
            "METEOR-001",
            &reading,
            Some(&fault),
            None,
            1_700_000_000_000,
            3,
        )
        .expect("serialize");
        let v = parse(&bytes);

        assert_eq!(v["high_voltage_protection"], 1);
        assert_eq!(v["mosfet_fault"], 1);
        // Every other flag present as an explicit 0.
        assert_eq!(v["low_voltage_protection"], 0);
        assert_eq!(v["short_circuit_protection"], 0);
        assert_eq!(v["charge_current_warning"], 0);
    }

    #[test]
    fn num_cells_matches_the_emitted_cell_key_count() {
        let reading = sample_reading();
        let bytes = battery_payload("METEOR-001", &reading, None, None, 0, 0).expect("serialize");
        let v = parse(&bytes);
        let num_cells = v["num_cells"].as_u64().unwrap() as usize;
        for i in 0..num_cells {
            assert!(v.get(format!("cell_{i}_voltage")).is_some());
        }
        assert!(v.get(format!("cell_{num_cells}_voltage")).is_none());
    }
}
