use super::prelude::*;
use std::time::Duration;

impl LocationProvider for ModemManagerProvider {
    /// Runs `<binary> -m any --location-get -J`, parsing its JSON `modem.location.gps`
    /// sub-object. An absent binary (`ErrorKind::NotFound`) is reported as a normal tier failure,
    /// not propagated as a panic/hard-error — this is how the tier self-disables on a device with
    /// no ModemManager installed.
    async fn locate(&mut self) -> Result<LocationFix, &'static str> {
        let budget = Duration::from_millis(self.timeout_ms.max(1));
        let spawn = tokio::process::Command::new(&self.binary)
            .args(["-m", "any", "--location-get", "-J"])
            .output();
        let output = match tokio::time::timeout(budget, spawn).await {
            Ok(Ok(output)) => output,
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err("modem-manager: mmcli binary not found; tier disabled");
            }
            Ok(Err(_)) => return Err("modem-manager: failed to spawn mmcli"),
            Err(_) => return Err("modem-manager: mmcli timed out"),
        };
        if !output.status.success() {
            return Err("modem-manager: mmcli exited with a non-zero status");
        }
        let (lat, lon, alt_m) = parse_mmcli_location(&output.stdout)
            .ok_or("modem-manager: no GPS fix in mmcli output")?;
        Ok(LocationFix {
            lat,
            lon,
            alt_m,
            source: LocationSource::ModemManager,
            ts_ms: crate::locate::now_ms(),
        })
    }
}

/// Parse `mmcli -m any --location-get -J`'s JSON output, extracting the `modem.location.gps`
/// sub-object's `latitude`/`longitude`/`altitude` (each either a numeric-looking string, or a
/// literal `"--"` when unset).
///
/// PROVISIONAL (gap-closure #237, issue #241): the exact key layout is best-effort from
/// ModemManager CLI familiarity, not verified against a real modem or a captured fixture — no
/// reference prototype was available in this sandbox (`/home/user/meteor-adapter` absent; see
/// `docs/gap-analysis-meteor-parity.md`'s "Multi-tier location service" finding). If ModemManager's
/// real `-J` output nests the GPS fields differently, only this function needs to change.
fn parse_mmcli_location(raw: &[u8]) -> Option<(f64, f64, f64)> {
    let value: serde_json::Value = serde_json::from_slice(raw).ok()?;
    let gps = value.get("modem")?.get("location")?.get("gps")?;
    let lat = parse_numeric_field(gps.get("latitude")?)?;
    let lon = parse_numeric_field(gps.get("longitude")?)?;
    let alt = gps
        .get("altitude")
        .and_then(parse_numeric_field)
        .unwrap_or(0.0);
    Some((lat, lon, alt))
}

/// mmcli renders unset numeric fields as the literal string `"--"` rather than omitting the key or
/// using `null` — `"--".parse::<f64>()` fails cleanly, which is exactly the "no fix" signal this
/// function needs.
fn parse_numeric_field(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::String(s) => s.trim().parse::<f64>().ok(),
        serde_json::Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    // Canned mmcli JSON fixtures (PROVISIONAL shape — see `parse_mmcli_location`'s doc comment).
    const WITH_GPS_FIX: &str = r#"{
        "modem": {
            "location": {
                "gps": {
                    "latitude": "45.508840",
                    "longitude": "-73.587810",
                    "altitude": "42.0",
                    "utc": "123519.00"
                }
            }
        }
    }"#;

    const NO_FIX_YET: &str = r#"{
        "modem": {
            "location": {
                "gps": {
                    "latitude": "--",
                    "longitude": "--",
                    "altitude": "--",
                    "utc": "--"
                }
            }
        }
    }"#;

    const NUMERIC_FIELDS: &str = r#"{
        "modem": {
            "location": {
                "gps": {
                    "latitude": 45.5,
                    "longitude": -73.6
                }
            }
        }
    }"#;

    const NO_GPS_SUBOBJECT: &str = r#"{"modem": {"location": {"3gpp": {"lac": "1"}}}}"#;

    #[test]
    fn parses_a_real_fix() {
        let (lat, lon, alt) = parse_mmcli_location(WITH_GPS_FIX.as_bytes()).expect("has a gps fix");
        assert!((lat - 45.508840).abs() < 1e-6);
        assert!((lon - (-73.587810)).abs() < 1e-6);
        assert!((alt - 42.0).abs() < 1e-6);
    }

    #[test]
    fn dash_placeholder_fields_are_no_fix() {
        assert!(parse_mmcli_location(NO_FIX_YET.as_bytes()).is_none());
    }

    #[test]
    fn accepts_json_numbers_too_defaulting_missing_altitude_to_zero() {
        let (lat, lon, alt) = parse_mmcli_location(NUMERIC_FIELDS.as_bytes()).expect("has lat/lon");
        assert_eq!(lat, 45.5);
        assert_eq!(lon, -73.6);
        assert_eq!(alt, 0.0);
    }

    #[test]
    fn missing_gps_subobject_is_no_fix() {
        assert!(parse_mmcli_location(NO_GPS_SUBOBJECT.as_bytes()).is_none());
    }

    #[test]
    fn garbage_input_is_no_fix_not_a_panic() {
        assert!(parse_mmcli_location(b"not json").is_none());
    }
}
