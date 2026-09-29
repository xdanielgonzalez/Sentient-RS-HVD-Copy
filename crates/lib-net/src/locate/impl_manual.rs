use super::prelude::*;

impl LocationProvider for ManualProvider {
    /// Always succeeds — a manual override is, definitionally, always "acquired".
    async fn locate(&mut self) -> Result<LocationFix, &'static str> {
        Ok(LocationFix {
            lat: self.lat,
            lon: self.lon,
            alt_m: 0.0,
            source: LocationSource::Manual,
            ts_ms: crate::locate::now_ms(),
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn always_succeeds_with_the_configured_fix() {
        let mut p = ManualProvider {
            lat: 12.5,
            lon: -8.25,
        };
        let fix = p.locate().await.expect("manual tier never fails");
        assert_eq!(fix.lat, 12.5);
        assert_eq!(fix.lon, -8.25);
        assert_eq!(fix.source, LocationSource::Manual);
    }
}
