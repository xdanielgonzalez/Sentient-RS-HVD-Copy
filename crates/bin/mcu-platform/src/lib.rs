//! `bin-mcu-platform` — the Embassy MCU platform impl (EPIC #350, story #352): the seams
//! `impl_app::run`'s shared poll loop binds to on the Embassy-runtime MCU path, symmetric with
//! [`bin_rpi_platform::HostPlatform`] and (the now-superseded) `bin_zephyr_platform::ZephyrPlatform`
//! — depends on `spec_app` (the contract), never on `impl_app` (the policy), same ADR 0009
//! dependency inversion.
//!
//! ## Embassy-version-free by construction
//!
//! `bin/mcu-rp2350` (the real Embassy MCU binary this crate serves) lives in an EXCLUDED workspace
//! pinning `embassy-rp` 0.10 + `embassy-time` 0.5. THIS crate, by contrast, is a normal workspace
//! member (so the host `cargo build`/`clippy`/`test` gates cover its whole surface) — and the main
//! workspace already pins `embassy-time = "0.4"` for `lib-net`'s embassy-net spike.
//! `embassy-time-driver` carries a `links = "embassy-time"` key, so Cargo enforces exactly ONE
//! `embassy-time` version per dependency graph; this crate must therefore never pull `embassy-time`
//! (or any other `embassy-*` crate) transitively or directly. Every embassy-shaped piece is instead
//! a generic parameter the board crate (`bin/mcu-rp2350`) instantiates with its own concrete types:
//! - [`McuClock`] — the injected clock/sleep seam ([`types::McuClock::now`]/`sleep_ms`) stands in
//!   for `embassy_time::Instant`/`Timer`.
//! - [`lib_io::EmbassyUart`]'s own `T: embedded_hal_async::delay::DelayNs` generic (see that
//!   crate's `embassy_uart` module doc comment) stands in for `embassy_time::Timer` in the RS-485
//!   guard delays.
//! - [`ChannelPublisher`]'s `S: lib_net::channel::Sender<Item = MqttUnit>` generic stands in for a
//!   concrete `embassy_sync::channel::Sender` half (the `channel-embassy` feature's impl, enabled
//!   by `bin/mcu-rp2350` itself — never by this crate).
//!
//! ## Render-on-output, reused from lib-mapping (one wire format)
//!
//! [`McuPlatform::emit_reading`]/`emit_fault` mirror the render path
//! `bin_zephyr_platform::ZephyrPlatform`'s `RemoteClient` branch and
//! `bin_rpi_platform::HostPlatform` both use: [`lib_mapping::render::render_tms`]/`render_fault`
//! serialize straight into the shared `payload_buf`, and [`lib_mapping::sink::publish_rendered`]
//! builds the topic + publishes + the outcome is recorded into [`spec_app::Health`] via
//! `Health::record_publish` — no new serialization code, no TMS struct ever materialized on this
//! crate's own stack. The MCU never polls an inverter (same doctrine as Zephyr), so
//! `emit_inverter_reading`/`emit_inverter_fault`/`on_cycle`/`reconnect` all inherit
//! [`spec_app::Platform`]'s default no-ops.
#![no_std]

pub mod types;

mod impl_channel_publisher;
// The `RailMonitor` impl for the newtype wrapper around `lib_io::ina3221::Ina3221` — see
// `types::ina3221_rail_monitor`'s doc comment for why this lives here (orphan-rule workaround).
#[cfg(feature = "power")]
mod impl_ina3221_rail_monitor;
mod impl_mcu_platform;
// WP3 (delight-features): `impl Platform for MeshPlatform<..>` — see `types::mesh_platform`'s doc
// comment for why this is a decorator over `Platform` rather than a field on `McuPlatform` itself.
#[cfg(feature = "mesh")]
mod impl_mesh_platform;

mod prelude;

pub use types::*;
// The free render/publish helper `McuPlatform::emit_rail_draw` wraps — exported so
// `ns::boot_i2c::rail_task` (a different crate) can call it directly with its own `Publisher`
// handle. See `impl_mcu_platform.rs`'s doc comment on the fn itself. Diagnostic-only: publishes to
// `diag/rails`, never through `lib_mapping`/TMS.
#[cfg(feature = "power")]
pub use impl_mcu_platform::{publish_rail_draw, DIAG_RAILS_TOPIC};
