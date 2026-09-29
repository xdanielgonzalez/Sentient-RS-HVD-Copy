//! The Raspberry Pi platform: embedded rumqttd broker + tokio-serial transport + tokio clock.
//! Constructed from config by [`HostPlatform::build_port`].
//!
//! Ports & adapters (ADR 0009, `docs/decisions/0009-spec-app-ports-and-adapters.md`): this is the
//! RPi (Yocto/std+tokio) adapter — the symmetric peer of `bin-zephyr-platform` — implementing the
//! `spec_app::Platform` port. It depends on `spec-app` (the contract) but never on `impl-app` (the
//! policy): the poll → map → publish loop in `impl_app::run`/`impl_app::run_inverter` is handed
//! this crate's [`HostPlatform`] by the composition root (`bin-rpi`), never the other way around.
#![forbid(unsafe_code)]

mod prelude;
mod types;

mod impl_capture_serial;
mod impl_cloud_check_info;
mod impl_cloud_creds_present;
mod impl_cycle_pulse;
mod impl_dds_tap;
mod impl_disk_tap;
mod impl_emit_cadence;
mod impl_greengrass_tap;
mod impl_host_ctx;
mod impl_host_platform;
mod impl_host_tap;
mod impl_mesh_tap;
mod impl_mosquitto_bridge_renderable;
mod impl_sink_set;
mod impl_timing_profile;
mod impl_watchdog_agg;

pub use types::*;

/// Signal `READY=1` to systemd once every configured bus's platform is built and every poll loop
/// is about to start. No-op outside a `Type=notify` unit. Call from the binary's `main`/
/// `runtime::supervise` after every [`HostPlatform::build_port`].
pub fn notify_ready() {
    let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Ready]);
}

/// Signal `STOPPING=1` to systemd at the start of a graceful shutdown (before aborting the poll
/// loops). No-op outside a `Type=notify` unit.
pub fn notify_stopping() {
    let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Stopping]);
}
