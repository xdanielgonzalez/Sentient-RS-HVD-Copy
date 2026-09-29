use super::prelude::*;
use std::time::Duration;

impl LocationProvider for GpsNmeaProvider {
    /// Open `self.port` fresh, read lines until a checksum-valid `$--GGA`/`$--RMC` sentence with
    /// an actual fix (not "no fix yet") turns up, or `self.read_timeout_ms` elapses.
    async fn locate(&mut self) -> Result<LocationFix, &'static str> {
        let budget = Duration::from_millis(self.read_timeout_ms.max(1));
        let stream = tokio_serial::new(&self.port, self.baud)
            .timeout(budget)
            .open_native_async()
            .map_err(|_| "gps-nmea: failed to open serial port")?;

        tokio::time::timeout(budget, read_until_fix(stream))
            .await
            .map_err(|_| "gps-nmea: timed out waiting for a valid fix")?
            .map(|(lat, lon, alt_m)| LocationFix {
                lat,
                lon,
                alt_m,
                source: LocationSource::GpsNmea,
                ts_ms: crate::locate::now_ms(),
            })
    }
}

/// Read lines off `stream` until a valid GGA or RMC fix is found, or the underlying reader
/// errors/closes. Bounded by the caller's `tokio::time::timeout` — this loop itself never gives up
/// on its own.
async fn read_until_fix(
    stream: tokio_serial::SerialStream,
) -> Result<(f64, f64, f64), &'static str> {
    let mut reader = tokio::io::BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader
            .read_line(&mut line)
            .await
            .map_err(|_| "gps-nmea: serial read error")?;
        if n == 0 {
            return Err("gps-nmea: serial port closed");
        }
        if let Some(fix) = parse_sentence(&line) {
            return Ok(fix);
        }
    }
}

/// One checksum-valid NMEA-0183 sentence -> `(lat, lon, alt_m)`, or `None` if the line is
/// malformed, not a GGA/RMC sentence, or reports no fix yet (`GGA` fix-quality 0 / `RMC` status
/// `V`). `alt_m` is `0.0` for RMC, which carries no altitude field.
fn parse_sentence(raw: &str) -> Option<(f64, f64, f64)> {
    if !checksum_ok(raw) {
        return None;
    }
    let fields = sentence_fields(raw)?;
    let kind = fields.first().copied().and_then(sentence_kind)?;
    match kind {
        SentenceKind::Gga => parse_gga(&fields),
        SentenceKind::Rmc => parse_rmc(&fields).map(|(lat, lon)| (lat, lon, 0.0)),
    }
}

enum SentenceKind {
    Gga,
    Rmc,
}

/// Talker-agnostic sentence-type match: the talker ID prefix (`GP`/`GN`/`GL`/`GA`/`GB`/…) varies
/// by constellation/receiver, but the trailing 3 characters (`GGA`/`RMC`) don't.
fn sentence_kind(first_field: &str) -> Option<SentenceKind> {
    if first_field.ends_with("GGA") {
        Some(SentenceKind::Gga)
    } else if first_field.ends_with("RMC") {
        Some(SentenceKind::Rmc)
    } else {
        None
    }
}

/// `$TALKERSSS,f1,f2,...*CS` -> the comma-split fields (field 0 is `$TALKERSSS`), dropping the
/// checksum suffix. `None` if there's no `$`/`*` framing at all.
fn sentence_fields(raw: &str) -> Option<Vec<&str>> {
    let body = raw.trim().strip_prefix('$')?;
    let data = body.split('*').next()?;
    Some(data.split(',').collect())
}

/// NMEA-0183's checksum: the XOR of every byte between `$` and `*`, as two uppercase/lowercase hex
/// digits. Rejects a line with no `*CS` suffix outright — a checksum-less "sentence" is far more
/// likely line noise (a half-received fragment, a non-NMEA log line sharing the port) than a real
/// fix.
fn checksum_ok(raw: &str) -> bool {
    let Some(body) = raw.trim().strip_prefix('$') else {
        return false;
    };
    let Some(star) = body.find('*') else {
        return false;
    };
    let (data, tail) = body.split_at(star);
    let given = &tail[1..];
    if given.len() < 2 {
        return false;
    }
    let computed = data.bytes().fold(0u8, |acc, b| acc ^ b);
    u8::from_str_radix(&given[..2], 16)
        .map(|v| v == computed)
        .unwrap_or(false)
}

/// `ddmm.mmmm`/`N|S` (`deg_digits = 2`) or `dddmm.mmmm`/`E|W` (`deg_digits = 3`) -> signed decimal
/// degrees.
fn parse_coord(value: &str, hemi: &str, deg_digits: usize) -> Option<f64> {
    if value.len() <= deg_digits || hemi.is_empty() {
        return None;
    }
    let deg: f64 = value[..deg_digits].parse().ok()?;
    let min: f64 = value[deg_digits..].parse().ok()?;
    let decimal = deg + min / 60.0;
    Some(
        if hemi.eq_ignore_ascii_case("S") || hemi.eq_ignore_ascii_case("W") {
            -decimal
        } else {
            decimal
        },
    )
}

/// `$--GGA,time,lat,N/S,lon,E/W,quality,sats,hdop,alt,M,...` -> `(lat, lon, alt_m)`, or `None` on
/// `quality == 0` ("no fix").
fn parse_gga(fields: &[&str]) -> Option<(f64, f64, f64)> {
    if fields.len() < 10 {
        return None;
    }
    let quality: u8 = fields[6].parse().ok()?;
    if quality == 0 {
        return None;
    }
    let lat = parse_coord(fields[2], fields[3], 2)?;
    let lon = parse_coord(fields[4], fields[5], 3)?;
    let alt = fields[9].parse::<f64>().unwrap_or(0.0);
    Some((lat, lon, alt))
}

/// `$--RMC,time,status(A/V),lat,N/S,lon,E/W,...` -> `(lat, lon)`, or `None` on status `V` ("void").
fn parse_rmc(fields: &[&str]) -> Option<(f64, f64)> {
    if fields.len() < 7 {
        return None;
    }
    if fields[2] != "A" {
        return None;
    }
    let lat = parse_coord(fields[3], fields[4], 2)?;
    let lon = parse_coord(fields[5], fields[6], 3)?;
    Some((lat, lon))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    // Real-world GGA/RMC fixtures (checksums verified against the NMEA-0183 XOR algorithm).
    const GGA_FIX: &str = "$GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,*47\r\n";
    const GGA_NO_FIX: &str =
        "$GPGGA,123519,4807.038,N,01131.000,E,0,08,0.9,545.4,M,46.9,M,,*46\r\n";
    const RMC_FIX: &str =
        "$GPRMC,123519,A,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W*6A\r\n";
    const RMC_VOID: &str =
        "$GPRMC,123519,V,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W*7D\r\n";
    // GN talker (combined GNSS receiver) — same trailing sentence type.
    const GNRMC_FIX: &str =
        "$GNRMC,123519,A,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W*74\r\n";

    #[test]
    fn checksum_accepts_a_valid_sentence_and_rejects_a_corrupted_one() {
        assert!(checksum_ok(GGA_FIX));
        let corrupted = GGA_FIX.replace("4807.038", "4807.039");
        assert!(!checksum_ok(&corrupted));
        assert!(!checksum_ok("no dollar or star here"));
    }

    #[test]
    fn gga_with_a_fix_parses_lat_lon_alt() {
        let (lat, lon, alt) = parse_sentence(GGA_FIX).expect("GGA_FIX has a fix");
        assert!((lat - 48.1173).abs() < 1e-3);
        assert!((lon - 11.5167).abs() < 1e-3);
        assert!((alt - 545.4).abs() < 1e-6);
    }

    #[test]
    fn gga_with_no_fix_is_rejected() {
        assert!(parse_sentence(GGA_NO_FIX).is_none());
    }

    #[test]
    fn rmc_active_parses_lat_lon_with_zero_altitude() {
        let (lat, lon, alt) = parse_sentence(RMC_FIX).expect("RMC_FIX is active");
        assert!((lat - 48.1173).abs() < 1e-3);
        assert!((lon - 11.5167).abs() < 1e-3);
        assert_eq!(alt, 0.0);
    }

    #[test]
    fn rmc_void_is_rejected() {
        assert!(parse_sentence(RMC_VOID).is_none());
    }

    #[test]
    fn gn_talker_is_accepted_like_gp() {
        assert!(parse_sentence(GNRMC_FIX).is_some());
    }

    #[test]
    fn southern_and_western_hemispheres_negate() {
        let lat = parse_coord("4807.038", "S", 2).expect("valid");
        let lon = parse_coord("01131.000", "W", 3).expect("valid");
        assert!(lat < 0.0);
        assert!(lon < 0.0);
    }
}
