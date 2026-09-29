use lib_net::locate::LocationFix;
use lib_net::mesh::RadioHandle;
use lib_parse::canonical::CanonicalReading;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Meshtastic mesh-radio tap — offers each [`super::tap_event::TapEvent`] to the radio task
/// (`lib_net::mesh::run_radio`) over `handle`. `max_payload` carries the configured on-air budget
/// (mirrors `lib_net::mesh::MeshConfig::max_payload`) forward to [`lib_net::mesh::condense`] at offer time,
/// since this tap does not otherwise hold the mesh config.
#[derive(Clone)]
pub struct MeshTap {
    pub handle: RadioHandle,
    pub max_payload: usize,
    /// #387 fault forwarding: the most recently seen [`CanonicalReading`] per DIP address
    /// (`device_address`), so a `TapEvent::Fault` (which carries no reading of its own) has
    /// something to pair with — `condense`'s `CondenseInput::reading` field is mandatory (see that
    /// struct's doc comment: "this is what makes a frame worth sending at all"). Mirrors
    /// `GreengrassTap::fault_cache`'s shape/rationale, just the other direction (reading cached,
    /// fault arriving): `Arc<Mutex<_>>` (not a plain field) because `SinkSet`/`HostTap` derive
    /// `Clone` and `bin-rpi::runtime` clones the tap per bus — a plain field would give each clone
    /// its own divergent cache, and the cache must persist across cycles the same way
    /// `fault_cache` does. A DIP has no entry until its first `Reading`; a `Fault` for a DIP
    /// nothing has been read from yet is dropped rather than sent with no reading to attach it to.
    pub last_reading: Arc<Mutex<HashMap<u8, CanonicalReading>>>,
    /// Location watch receiver (`lib_net::locate::run_refresher`'s `tx` side), mirroring
    /// `GreengrassTap::location_rx` exactly — always present (falls back to an always-`None`
    /// channel when `[location]` isn't configured; see `bin-rpi::runtime::build_taps`), so a
    /// condensed frame can carry `CondenseInput::loc` whenever a fix is available.
    pub location_rx: tokio::sync::watch::Receiver<Option<LocationFix>>,
}
