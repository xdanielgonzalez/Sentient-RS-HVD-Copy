use super::prelude::*;
use lib_mapping::legacy::{battery_payload, inverter_payload, LegacyLocationFix};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Wall-clock milliseconds "now", for the merged payload's `timestamp`/`location_timestamp`.
/// Mirrors `impl_host_tap.rs`'s helper of the same name (D1/D3: the legacy payload's timestamp is
/// wall-clock `SystemTime::now()`, NOT the poll loop's monotonic `ClockMonotonic` — an ISO-8601
/// `datetime.utcnow()`-shaped field must reflect real calendar time, not seconds-since-boot).
/// Saturates to 0 rather than panicking if the clock is somehow before the epoch.
fn wall_clock_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// `lib_net::locate::LocationSource` -> the legacy JSON's snake_case source string (baseline-verified
/// set: `"manual"` / `"gps_nmea"` / `"modem_manager"` / `"ip_geo"`). Lives here (not in
/// `lib-mapping`) because `lib-mapping` must not depend on `lib-locate` — see
/// `lib_mapping::legacy::LegacyLocationFix`'s doc comment.
fn source_name(source: lib_net::locate::LocationSource) -> &'static str {
    match source {
        lib_net::locate::LocationSource::Manual => "manual",
        lib_net::locate::LocationSource::GpsNmea => "gps_nmea",
        lib_net::locate::LocationSource::ModemManager => "modem_manager",
        lib_net::locate::LocationSource::IpGeo => "ip_geo",
    }
}

impl GreengrassTap {
    /// Build a `GreengrassTap` from a freshly minted broker publisher handle (see the struct doc
    /// comment for why it must be this tap's own, not the bus's primary publisher), the configured
    /// unit-level device id, its baseline `device_type` (`"battery"` for the Seplos path), and a
    /// location watch receiver (`lib_net::locate::run_refresher`'s `tx` side; see
    /// `bin-rpi::runtime::supervise`). Starts with an empty fault cache — no DIP has fault flags
    /// until its first successful fault poll.
    pub fn new(
        publisher: BrokerPublisher,
        device_id: impl Into<String>,
        device_type: impl Into<String>,
        location_rx: tokio::sync::watch::Receiver<Option<lib_net::locate::LocationFix>>,
    ) -> Self {
        GreengrassTap {
            enabled: true,
            publisher: Arc::new(Mutex::new(publisher)),
            device_id: device_id.into(),
            device_type: device_type.into(),
            fault_cache: Arc::new(Mutex::new(HashMap::new())),
            inverter_fault_cache: Arc::new(Mutex::new(None)),
            location_rx,
        }
    }

    /// Offer `event` to this tap. Infallible by contract, same as every other `HostTap` variant
    /// (see `SinkSet::offer`'s doc comment) — a render or publish failure is logged at `warn` and
    /// dropped, never propagated to the poll loop.
    ///
    /// - `Reading` — render the baseline-verified merged battery JSON (the reading merged with the
    ///   most recently cached fault for its DIP) and publish it to
    ///   `telemetry/{device_type}/{device_id}`.
    /// - `Fault` — update the fault cache for that DIP only (D2 fault-merge cache); the flags land
    ///   in the SAME message as the NEXT reading, not a message of their own. This matches the
    ///   baseline, which merges the most recently *cached* fault flags into the reading payload.
    /// - `Published` — ignored: Greengrass gets its OWN flat rendering of the canonical
    ///   `reading`/`fault`, not the TMS wire bytes.
    ///
    /// **Wiring note:** `HostTap::offer`'s `Greengrass` arm (`impl_host_tap.rs`) forwards here
    /// exactly as `tap.offer(event)` once `tap.enabled` — the dispatch this closes (gap-closure
    /// #269) was previously a no-op stub.
    pub fn offer(&mut self, event: &TapEvent<'_>) {
        if !self.enabled {
            return;
        }
        match *event {
            TapEvent::Reading { reading, .. } => self.publish_reading(reading),
            TapEvent::Fault { fault, .. } => self.cache_fault(fault),
            // The RS-232 Voltronic leg (D10b): render the merged flat legacy INVERTER JSON and
            // publish to `telemetry/inverter/{device_id}` (an `InverterReading`), or update the
            // inverter fault-merge cache (an `InverterFault`) so the flags land in the SAME message
            // as the next inverter reading — the exact merge-on-success shape as the battery path.
            TapEvent::InverterReading { reading, .. } => self.publish_inverter_reading(reading),
            TapEvent::InverterFault { fault, .. } => self.cache_inverter_fault(fault),
            TapEvent::Published { .. } => {}
        }
    }

    /// D2: cache the fault keyed by its DIP address. No publish — the flags merge into the next
    /// reading for that DIP (see [`GreengrassTap::publish_reading`]).
    fn cache_fault(&self, fault: &CanonicalFault) {
        let mut cache = self
            .fault_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = cache.insert(fault.device_address, *fault);
    }

    /// Render the merged battery payload (reading + the cached fault for this reading's DIP, if any)
    /// and publish it to `telemetry/{device_type}/{device_id}`. The fault is looked up (and copied
    /// out) under the cache lock so no borrow of the cache escapes it.
    fn publish_reading(&self, reading: &CanonicalReading) {
        let location = *self.location_rx.borrow();
        let cached_fault = {
            let cache = self
                .fault_cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            cache.get(&reading.device_address).copied()
        };
        let payload = match self.render_reading(
            reading,
            cached_fault.as_ref(),
            location.as_ref(),
            wall_clock_ms(),
        ) {
            Ok(bytes) => bytes,
            Err(e) => {
                log::warn!("greengrass tap: failed to render legacy reading JSON: {e}");
                return;
            }
        };

        let topic = format!("telemetry/{}/{}", self.device_type, self.device_id);
        let mut publisher = self
            .publisher
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Err(e) = publisher.publish(&topic, &payload) {
            log::warn!("greengrass tap: publish failed on {topic}: {e}");
        }
    }

    fn render_reading(
        &self,
        reading: &CanonicalReading,
        fault: Option<&CanonicalFault>,
        location: Option<&lib_net::locate::LocationFix>,
        wall_ms: u64,
    ) -> serde_json::Result<Vec<u8>> {
        let loc = location.map(|fix| LegacyLocationFix {
            latitude: fix.lat,
            longitude: fix.lon,
            source: source_name(fix.source),
            ts_ms: fix.ts_ms,
        });
        battery_payload(
            &self.device_id,
            reading,
            fault,
            loc.as_ref(),
            wall_ms,
            reading.device_address,
        )
    }

    /// D2 (inverter): cache the QPIWS fault snapshot. No publish — the flags merge into the next
    /// inverter reading (see [`GreengrassTap::publish_inverter_reading`]). Keyed by the whole tap (a
    /// single inverter per bus — no DIP address).
    fn cache_inverter_fault(&self, fault: &CanonicalInverterFault) {
        let mut cache = self
            .inverter_fault_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *cache = Some(*fault);
    }

    /// Render the merged legacy inverter payload (reading + the cached inverter fault, if any) and
    /// publish it to `telemetry/{device_type}/{device_id}` (`device_type` = `"inverter"` for this
    /// tap). The cached fault is copied out under the lock so no borrow of the cache escapes it —
    /// mirrors [`GreengrassTap::publish_reading`] exactly, for the Voltronic leg.
    fn publish_inverter_reading(&self, reading: &CanonicalInverterReading) {
        let location = *self.location_rx.borrow();
        let cached_fault = {
            let cache = self
                .inverter_fault_cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *cache
        };
        let payload = match self.render_inverter_reading(
            reading,
            cached_fault.as_ref(),
            location.as_ref(),
            wall_clock_ms(),
        ) {
            Ok(bytes) => bytes,
            Err(e) => {
                log::warn!("greengrass tap: failed to render legacy inverter JSON: {e}");
                return;
            }
        };

        let topic = format!("telemetry/{}/{}", self.device_type, self.device_id);
        let mut publisher = self
            .publisher
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Err(e) = publisher.publish(&topic, &payload) {
            log::warn!("greengrass tap: publish failed on {topic}: {e}");
        }
    }

    fn render_inverter_reading(
        &self,
        reading: &CanonicalInverterReading,
        fault: Option<&CanonicalInverterFault>,
        location: Option<&lib_net::locate::LocationFix>,
        wall_ms: u64,
    ) -> serde_json::Result<Vec<u8>> {
        let loc = location.map(|fix| LegacyLocationFix {
            latitude: fix.lat,
            longitude: fix.lon,
            source: source_name(fix.source),
            ts_ms: fix.ts_ms,
        });
        inverter_payload(&self.device_id, reading, fault, loc.as_ref(), wall_ms)
    }
}
