//! Legacy AWS-Greengrass merged flat **inverter** payload (Scope D, D10b — the RS-232 Voltronic
//! leg's counterpart to [`super::battery_payload`]). Feature `legacy` (requires `std`).
//!
//! Mirrors `battery_payload` exactly in shape (base + flat location fields, then the device-specific
//! reading fields, then the cached fault flags merged into the SAME message) and topic namespace
//! (`telemetry/inverter/{device_id}`). The QPIGS/QMOD/QPIWS field names are the legacy baseline's own
//! (`example/aws/src/greengrass/telemetry/inverter/inverter_reader.py` + `config.yaml`), listed in
//! `docs`/the WS-CF brief's "Legacy inverter payload" section.
//!
//! **PROVISIONAL** — unlike `battery_payload` (baseline-VERIFIED against a checked-in reference),
//! the inverter baseline mapping is derived from the legacy Python + protocol docs with no real
//! Voltronic wire captures (decision 0001's open risk). Field names/scales are best-effort and
//! doc-commented at each site.

use serde_json::{Map, Value};

use lib_parse::canonical::{CanonicalInverterFault, CanonicalInverterReading, InverterMode};

use super::{iso8601_utc_ms, LegacyLocationFix};

/// The legacy `device_mode` string for a QMOD [`InverterMode`] (baseline spelling:
/// `power_on|standby|line_mode|battery_mode|fault|power_saving`; an unrecognized mode renders
/// `"unknown"` rather than being omitted).
fn device_mode_str(mode: InverterMode) -> &'static str {
    match mode {
        InverterMode::PowerOn => "power_on",
        InverterMode::Standby => "standby",
        InverterMode::Line => "line_mode",
        InverterMode::Battery => "battery_mode",
        InverterMode::Fault => "fault",
        InverterMode::PowerSaving => "power_saving",
        InverterMode::Unknown => "unknown",
    }
}

/// `(flag name, is set)` pairs for every QPIWS-derived inverter fault flag, in the canonical
/// struct's declared order. Walked by [`inverter_payload`] to merge 0/1 fault-flag keys plus the
/// summary `has_fault` into the payload (only once a fault poll has succeeded).
fn fault_flags(f: &CanonicalInverterFault) -> [(&'static str, bool); 6] {
    [
        ("line_fail", f.line_fail),
        ("overtemperature_fault", f.overtemperature_fault),
        ("fan_locked_fault", f.fan_locked_fault),
        ("battery_low_fault", f.battery_low_fault),
        ("overload_fault", f.overload_fault),
        ("output_short_fault", f.output_short_fault),
    ]
}

/// Build ONE merged flat inverter telemetry JSON payload for `telemetry/inverter/{device_id}`.
/// `device_id` is the caller's inverter identity (`{unit}-inv` — see
/// `bin-rpi::runtime`/D13); `fault` is `None` until a QPIWS poll has succeeded at least once (cache
/// semantics owned by the caller — see `impl_app::host::GreengrassTap`), matching `battery_payload`'s
/// merge-on-success behavior; while `None`, no fault-flag keys (nor `has_fault`) are emitted at all.
///
/// Field set (baseline QPIGS/QMOD/QPIRI fields):
/// - base: `timestamp` (ISO-8601 Z), `device_id`, `device_type` = `"inverter"`, flat `latitude`/
///   `longitude` (KEYS OMITTED when `loc` is `None`), `location_source`, `location_timestamp`
///   (identical treatment to `battery_payload`)
/// - QPIGS reading fields: `grid_voltage`, `grid_frequency`, `ac_output_voltage`,
///   `ac_output_frequency`, `ac_output_apparent_power`, `ac_output_active_power`, `bus_voltage`,
///   `battery_voltage`, `battery_charging_current`, `inverter_temperature`, `pv_current`,
///   `pv_voltage`, `battery_discharge_current`, `pv_charging_power`
/// - status bits as 0/1 ints: `load_on`, `charging`, `solar_charging`, `ac_charging`
/// - `device_mode` (QMOD string; see `device_mode_str`)
/// - when the reading carries QPIRI ratings: `battery_type`, `variant`, `output_mode`
/// - when `fault.is_some()`: the 6 fault flags as 0/1 ints plus `has_fault` (1 if any set)
pub fn inverter_payload(
    device_id: &str,
    r: &CanonicalInverterReading,
    fault: Option<&CanonicalInverterFault>,
    loc: Option<&LegacyLocationFix>,
    wall_ms: u64,
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
        Value::String("inverter".to_string()),
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
            // Same as `battery_payload`: omit latitude/longitude keys entirely (never JSON null)
            // when no fix is available; still emit location_source/location_timestamp.
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

    // QPIGS reading fields — engineering units (scaled from the canonical fixed-point raw units).
    m.insert(
        "grid_voltage".to_string(),
        Value::from(r.grid_voltage_dv as f64 * 0.1),
    );
    m.insert(
        "grid_frequency".to_string(),
        Value::from(r.grid_frequency_dhz as f64 * 0.1),
    );
    m.insert(
        "ac_output_voltage".to_string(),
        Value::from(r.ac_output_voltage_dv as f64 * 0.1),
    );
    m.insert(
        "ac_output_frequency".to_string(),
        Value::from(r.ac_output_frequency_dhz as f64 * 0.1),
    );
    m.insert(
        "ac_output_apparent_power".to_string(),
        Value::from(r.ac_output_apparent_power_va),
    );
    m.insert(
        "ac_output_active_power".to_string(),
        Value::from(r.ac_output_active_power_w),
    );
    m.insert("bus_voltage".to_string(), Value::from(r.bus_voltage_v));
    m.insert(
        "battery_voltage".to_string(),
        Value::from(r.battery_voltage_cv as f64 * 0.01),
    );
    m.insert(
        "battery_charging_current".to_string(),
        Value::from(r.battery_charging_current_a),
    );
    m.insert(
        "inverter_temperature".to_string(),
        Value::from(r.inverter_temperature_c),
    );
    m.insert(
        "pv_current".to_string(),
        Value::from(r.pv_current_da as f64 * 0.1),
    );
    m.insert(
        "pv_voltage".to_string(),
        Value::from(r.pv_voltage_dv as f64 * 0.1),
    );
    m.insert(
        "battery_discharge_current".to_string(),
        Value::from(r.battery_discharge_current_a),
    );
    m.insert(
        "pv_charging_power".to_string(),
        Value::from(r.pv_charging_power_w),
    );

    // Status bits as 0/1 ints (see `CanonicalInverterReading`'s accessors).
    m.insert("load_on".to_string(), Value::from(u8::from(r.load_on())));
    m.insert("charging".to_string(), Value::from(u8::from(r.charging())));
    m.insert(
        "solar_charging".to_string(),
        Value::from(u8::from(r.solar_charging())),
    );
    m.insert(
        "ac_charging".to_string(),
        Value::from(u8::from(r.ac_charging())),
    );

    m.insert(
        "device_mode".to_string(),
        Value::String(device_mode_str(r.mode).to_string()),
    );

    // QPIRI ratings — only when the reading carries them (the caller merges a cached QPIRI result).
    if let Some(ratings) = &r.ratings {
        m.insert(
            "battery_type".to_string(),
            Value::String(ratings.battery_type.as_str().to_string()),
        );
        m.insert(
            "variant".to_string(),
            Value::String(ratings.variant.as_str().to_string()),
        );
        m.insert(
            "output_mode".to_string(),
            Value::String(ratings.output_mode.as_str().to_string()),
        );
    }

    // QPIWS faults — merged only after a successful fault poll (cache semantics; see doc comment).
    if let Some(f) = fault {
        let mut any = false;
        for (name, set) in fault_flags(f) {
            any |= set;
            m.insert(name.to_string(), Value::from(u8::from(set)));
        }
        m.insert("has_fault".to_string(), Value::from(u8::from(any)));
    }

    serde_json::to_vec(&Value::Object(m))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use lib_parse::canonical::InverterRatings;

    fn sample() -> CanonicalInverterReading {
        let mut battery_type = heapless::String::new();
        battery_type.push_str("lithium").unwrap();
        let mut variant = heapless::String::new();
        variant.push_str("3000").unwrap();
        let mut output_mode = heapless::String::new();
        output_mode.push_str("single_phase").unwrap();
        CanonicalInverterReading {
            grid_voltage_dv: 2304,
            grid_frequency_dhz: 500,
            ac_output_voltage_dv: 2300,
            ac_output_frequency_dhz: 500,
            ac_output_apparent_power_va: 900,
            ac_output_active_power_w: 850,
            bus_voltage_v: 400,
            battery_voltage_cv: 5120,
            battery_charging_current_a: 8,
            battery_discharge_current_a: 0,
            inverter_temperature_c: 27,
            pv_current_da: 50,
            pv_voltage_dv: 3800,
            pv_charging_power_w: 1900,
            // bit4 load_on + bit2 charging + bit1 solar_charging set (0b0001_0110 = 22).
            status_bits: 0b0001_0110,
            mode: InverterMode::Line,
            ratings: Some(InverterRatings {
                battery_type,
                variant,
                output_mode,
            }),
        }
    }

    fn parse(bytes: &[u8]) -> Value {
        serde_json::from_slice(bytes).expect("valid json")
    }

    #[test]
    fn golden_inverter_json_without_fault_or_location() {
        let r = sample();
        let bytes = inverter_payload("METEOR-001-inv", &r, None, None, 1_700_000_000_000)
            .expect("serialize");
        let v = parse(&bytes);

        assert_eq!(v["device_id"], "METEOR-001-inv");
        assert_eq!(v["device_type"], "inverter");
        assert!(v["timestamp"].as_str().unwrap().ends_with('Z'));
        assert!(!v.as_object().unwrap().contains_key("latitude"));
        assert_eq!(v["location_source"], "unknown");

        assert!((v["grid_voltage"].as_f64().unwrap() - 230.4).abs() < 1e-9);
        assert!((v["grid_frequency"].as_f64().unwrap() - 50.0).abs() < 1e-9);
        assert!((v["ac_output_voltage"].as_f64().unwrap() - 230.0).abs() < 1e-9);
        assert_eq!(v["ac_output_apparent_power"], 900);
        assert_eq!(v["ac_output_active_power"], 850);
        assert_eq!(v["bus_voltage"], 400);
        assert!((v["battery_voltage"].as_f64().unwrap() - 51.20).abs() < 1e-9);
        assert_eq!(v["battery_charging_current"], 8);
        assert_eq!(v["inverter_temperature"], 27);
        assert!((v["pv_current"].as_f64().unwrap() - 5.0).abs() < 1e-9);
        assert!((v["pv_voltage"].as_f64().unwrap() - 380.0).abs() < 1e-9);
        assert_eq!(v["pv_charging_power"], 1900);

        assert_eq!(v["load_on"], 1);
        assert_eq!(v["charging"], 1);
        assert_eq!(v["solar_charging"], 1);
        assert_eq!(v["ac_charging"], 0);
        assert_eq!(v["device_mode"], "line_mode");

        assert_eq!(v["battery_type"], "lithium");
        assert_eq!(v["variant"], "3000");
        assert_eq!(v["output_mode"], "single_phase");

        // No fault polled yet -> no fault-flag keys (not even has_fault).
        assert!(v.get("line_fail").is_none());
        assert!(v.get("has_fault").is_none());
    }

    #[test]
    fn golden_inverter_json_with_fault_merges_all_flags_and_summary() {
        let r = sample();
        let fault = CanonicalInverterFault {
            overload_fault: true,
            ..Default::default()
        };
        let bytes = inverter_payload("METEOR-001-inv", &r, Some(&fault), None, 1_700_000_000_000)
            .expect("serialize");
        let v = parse(&bytes);
        assert_eq!(v["overload_fault"], 1);
        assert_eq!(v["line_fail"], 0);
        assert_eq!(v["output_short_fault"], 0);
        assert_eq!(v["has_fault"], 1);
    }

    #[test]
    fn all_clear_fault_reports_has_fault_zero() {
        let r = sample();
        let fault = CanonicalInverterFault::default();
        let bytes =
            inverter_payload("METEOR-001-inv", &r, Some(&fault), None, 0).expect("serialize");
        let v = parse(&bytes);
        assert_eq!(v["has_fault"], 0);
        assert_eq!(v["line_fail"], 0);
    }

    #[test]
    fn ratings_absent_omits_rating_keys() {
        let mut r = sample();
        r.ratings = None;
        let bytes = inverter_payload("METEOR-001-inv", &r, None, None, 0).expect("serialize");
        let v = parse(&bytes);
        assert!(v.get("battery_type").is_none());
        assert!(v.get("variant").is_none());
        assert!(v.get("output_mode").is_none());
    }

    #[test]
    fn location_fix_populates_flat_keys() {
        let r = sample();
        let loc = LegacyLocationFix {
            latitude: 37.7749,
            longitude: -122.4194,
            source: "gps_nmea",
            ts_ms: 1_700_000_000_500,
        };
        let bytes = inverter_payload("METEOR-001-inv", &r, None, Some(&loc), 1_700_000_000_000)
            .expect("serialize");
        let v = parse(&bytes);
        assert!((v["latitude"].as_f64().unwrap() - 37.7749).abs() < 1e-9);
        assert_eq!(v["location_source"], "gps_nmea");
    }
}
