use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use lib_dds::DdsHandle;
// `lib_mapping::schema::Identity` is `lib_tms::Identity` re-exported (`pub use lib_tms::*` in
// `lib_mapping::schema`) — this crate reaches `lib-tms` only transitively (via `lib-mapping`/
// `lib-dds`), so this type file names it through that existing path rather than adding a direct
// `lib-tms` dependency.
use lib_mapping::schema::Identity;

use super::emit_cadence::EmitCadence;

/// DDS/RTPS (MIL-STD-3071) secondary-publish tap (#324, WP6) — the DDS-transport counterpart of
/// [`super::greengrass_tap::GreengrassTap`]: on every [`super::tap_event::TapEvent::Reading`]/
/// [`super::tap_event::TapEvent::Fault`] it renders the same topics `HostPlatform::emit_reading`/
/// `emit_fault` publish over MQTT as XCDR2 (via `lib_mapping::render_cdr`) and enqueues each as a
/// [`lib_dds::DdsOutbound`] on `handle` — the bytes-in seam `lib_dds::run_dds_stack` drains. See
/// `crate::impl_dds_tap` for the render+cadence+enqueue behavior itself.
///
/// Holds its OWN [`EmitCadence`] (behind `Arc<Mutex<_>>` — `EmitCadence` is not `Clone`, and
/// `SinkSet`/`HostTap` derive `Clone`), independent of the bus's `HostPlatform::cadence`: the DDS
/// rate-limit/on-change windows run on their own clock, matching MQTT's cadence semantics
/// (`PowerHardwareUpdate`/`StorageUpdate` rate-limited to at-most-once-per-10 s;
/// `ActiveDiagnosticState` at-start + on-change + a 30 s safety net) without sharing state with the
/// MQTT path — a DDS-side drop never suppresses (or is suppressed by) an MQTT publish, or vice
/// versa.
#[derive(Clone)]
pub struct DdsTap {
    /// The clonable, bounded producer feeding `lib_dds::run_dds_stack`.
    pub handle: DdsHandle,
    /// This bus's derived TMS identity (D13) — the `DdsOutbound::device_id` / DDS instance key /
    /// `USER_DATA` for every payload this tap enqueues.
    pub device_id: Identity,
    /// This tap's own rate-limit/on-change gate state (see the struct doc comment).
    pub cadence: Arc<Mutex<EmitCadence>>,
    /// Count of enqueues dropped because `handle`'s bounded queue was full (or the DDS stack task
    /// is gone) — mirrors `DiskTap`/`MeshTap`'s `dropped` counter (`lib_dds::DdsHandle` itself
    /// carries no counter; this tap owns its own, per `impl_host_tap.rs`'s established pattern).
    pub dropped: Arc<AtomicU64>,
}
