use lib_net::channel::Sender;
use lib_parse::canonical::CanonicalReading;
use spec_app::Platform;

use super::mesh_unit::MeshUnit;

/// Decorator over any [`Platform`] impl that additionally condenses each emitted reading/fault and
/// forwards it to a Meshtastic mesh-radio channel (WP3, delight-features).
///
/// **Why a decorator, not a field on [`super::McuPlatform`] itself.** `McuPlatform`'s struct
/// literal (`McuPlatform { transport, publisher, clock }`) is built directly — no `new` — at
/// `ns/src/boot_seplos.rs::build_platform`, a file outside this work package's ownership (only ONE
/// agent may touch `ns/src/main.rs` this wave, and `boot_seplos.rs` isn't even that agent's file).
/// Adding a field to `McuPlatform` would require editing that struct literal to supply it, which
/// this work package cannot do. Wrapping at the `Platform` level sidesteps that entirely:
/// `impl_app::run<P: Platform>` is generic, so `ns/src/main.rs::run_task` (which this crate's
/// consumer DOES own) can wrap the base platform in `MeshPlatform` right before handing it to
/// `run`, with zero change to `McuPlatform`'s own fields or construction site.
///
/// **Fail-open, same posture as the W5500 leg.** A full/closed mesh channel, or a radio that never
/// drains it, is counted and dropped — never surfaced as a `Health`/`Platform` failure. The mesh
/// radio is a delight-feature secondary path; it must never take the primary MQTT telemetry mission
/// down.
pub struct MeshPlatform<Inner: Platform, S: Sender<Item = MeshUnit>> {
    pub inner: Inner,
    pub mesh: S,
    /// The MCU-side equivalent of the #387 fault-forwarding gap: [`Platform::emit_fault`] receives
    /// only a `&CanonicalFault`, no reading (see that trait method's doc comment) — but
    /// `lib_net::mesh::CondenseInput::reading` is mandatory, since a frame needs a reading to be
    /// worth sending at all. The most recently emitted reading is cached here so a fault that
    /// arrives afterward has something to pair with.
    ///
    /// Mirrors `bin-rpi-platform::MeshTap::last_reading`'s rationale, but a single slot rather than
    /// a per-DIP map: `ns`'s `RunConfig::dip_addresses` polls exactly one Seplos address, so there
    /// is only ever one reading to cache (unlike the Pi, which may poll multiple buses).
    pub last_reading: Option<CanonicalReading>,
}
