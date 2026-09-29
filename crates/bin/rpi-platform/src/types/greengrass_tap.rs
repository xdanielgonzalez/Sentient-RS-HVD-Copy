use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use lib_mqtt::broker::BrokerPublisher;
use lib_net::locate::LocationFix;
use lib_parse::canonical::{CanonicalFault, CanonicalInverterFault};
use tokio::sync::watch;

/// Greengrass backwards-compat tap (Scope D, closes #52-#54, gap-closure #269) — the AWS
/// legacy-pipeline route: on every [`super::tap_event::TapEvent::Reading`] it renders ONE merged
/// flat legacy JSON payload (`lib_mapping::legacy::battery_payload`, feature `legacy`) — the
/// current reading merged with the most recently *cached* fault flags for that DIP — and publishes
/// it to `telemetry/{device_type}/{device_id}` on the embedded broker, independent of (never
/// gating or gated by) the primary `tms/#` publish. `TapEvent::Fault` events update the fault
/// cache only (no publish); the flags land in the SAME message as the next reading, matching the
/// baseline's merge-on-success behavior. mosquitto Bridge 3 (`distro/deploy/mosquitto.conf`,
/// rendered by `crate::mosquitto_bridge::render_greengrass_bridge` in `bin-rpi`) carries
/// `telemetry/#` out to the AWS Greengrass core over mTLS. See `crate::impl_greengrass_tap`
/// for the render+cache+publish behavior itself.
///
/// Holds its **own** [`BrokerPublisher`] — a second `LocalBroker::publisher()` handle, not shared
/// with the bus's primary TMS publisher — so a Greengrass-side publish failure can never perturb
/// the primary publish path's `Health::record_publish` accounting (see `TapEvent`'s doc comment:
/// taps are observers of the primary path, never wired into its health). `BrokerPublisher` does
/// not itself implement `Clone` (its `publish` needs `&mut self`, and cloning would require
/// exposing its private link internals outside `lib-mqtt`) — wrapping it in `Arc<Mutex<_>>` here
/// keeps `GreengrassTap` (and therefore `HostTap`/`SinkSet`, both `#[derive(Clone)]`) cheaply
/// `Clone`-able without changing `BrokerPublisher`'s own shape.
#[derive(Clone)]
pub struct GreengrassTap {
    /// Runtime kill-switch, independent of a `[greengrass]` config section being present at all
    /// (preserves the pre-#53 `enabled` field callers already construct/read).
    pub enabled: bool,
    /// This tap's own broker publisher handle (see the struct doc comment).
    pub publisher: Arc<Mutex<BrokerPublisher>>,
    /// The unit-level device identity stamped into both the `telemetry/{device_type}/{device_id}`
    /// topic AND the merged payload's own `device_id` field
    /// (`lib_mapping::legacy::battery_payload`'s baseline-verified shape takes this identity
    /// directly — see `impl_greengrass_tap.rs`'s doc comment).
    pub device_id: String,
    /// The baseline `device_type` segment of the publish topic and the payload's `device_type`
    /// field — `"battery"` for the Seplos path (the only producer this wave; the Voltronic
    /// `"inverter"` identity lands a later wave). Interim single-identity per tap: per-bus identity
    /// is Wave 3 (see `bin-rpi::runtime::build_taps`).
    pub device_type: String,
    /// D2 fault-merge cache: the most recently polled [`CanonicalFault`] per DIP address. A
    /// `Reading` merges the cached fault for its own DIP into the published payload; a `Fault`
    /// event replaces the entry. A DIP stays ABSENT until its first successful fault poll, so early
    /// readings emit no fault-flag keys at all (baseline merge-on-success). `Arc<Mutex<_>>` (not a
    /// plain field) because `SinkSet`/`HostTap` derive `Clone` and `bin-rpi::runtime` clones the
    /// tap per bus — a plain field would give each clone its own divergent cache, and the cache
    /// must persist across cycles (`run_cycle` emits `Reading` every cycle but `Fault` only every
    /// `fault_every` cycles).
    pub fault_cache: Arc<Mutex<HashMap<u8, CanonicalFault>>>,
    /// D2 inverter fault-merge cache: the most recently polled [`CanonicalInverterFault`], keyed by
    /// the whole tap (an inverter is a single device per bus — no DIP address, so a plain `Option`,
    /// not a per-address `HashMap` like `fault_cache`). Parallel to `fault_cache` for the RS-232
    /// Voltronic leg (D13): an `InverterReading` merges the cached inverter fault into the published
    /// `telemetry/inverter/{id}` payload; an `InverterFault` replaces the entry. `None` until the
    /// first successful QPIWS poll. Only ever populated on a `device_type = "inverter"` tap (a
    /// battery tap never sees `InverterFault` events), but present on every tap so the struct shape
    /// stays uniform — an unused cache simply stays `None`.
    pub inverter_fault_cache: Arc<Mutex<Option<CanonicalInverterFault>>>,
    /// Best-effort location tag, refreshed by `lib_net::locate::run_refresher` — `None` until the
    /// first successful fix (or forever, on a device with no `[location]` chain configured).
    pub location_rx: watch::Receiver<Option<LocationFix>>,
}
