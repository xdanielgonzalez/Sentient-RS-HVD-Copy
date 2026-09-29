//! Private glob-import surface for this crate's split files — `impl_*.rs` ONLY (per CLAUDE.md's
//! Code Organization rule: type files under `types/` import directly, never this prelude, so the
//! `types <-> prelude` glob cycle never forms).

pub use super::types::*;

pub use embedded_hal::digital::OutputPin;
pub use embedded_hal_async::delay::DelayNs;
pub use embedded_io_async::{Read, Write};

pub use lib_io::embassy_uart::EmbassyUart;
pub use lib_mapping::render::{render_fault, render_storage_update, render_tms, TmsTopic};
pub use lib_mapping::schema::{topics, ClockMonotonic, Identity};
pub use lib_mapping::sink::{publish_rendered, PublishError, Publisher};
pub use lib_net::channel::message::{MQTT_PAYLOAD, MQTT_TOPIC};
pub use lib_net::channel::{MqttUnit, Sender, TrySendError};
pub use lib_parse::canonical::{CanonicalFault, CanonicalReading};
pub use spec_app::{Health, Platform, RunConfig};

// WP3 (delight-features) — `impl_mesh_platform.rs` only. `lib_net::mesh` (the no_std codec) and
// the inverter `Platform` method types (needed only because `MeshPlatform`'s decorator impl,
// unlike `McuPlatform`'s own, explicitly delegates EVERY `Platform` method rather than relying on
// the trait's default no-ops — see that impl's doc comment) are gated behind the same `mesh`
// feature so a non-mesh build never sees an unused-import warning for them.
#[cfg(feature = "mesh")]
pub use lib_net::mesh::{condense, CondenseInput};
#[cfg(feature = "mesh")]
pub use lib_parse::canonical::{CanonicalInverterFault, CanonicalInverterReading, InverterMode};
// `DeviceRailReading` is needed by BOTH `mesh` (`MeshPlatform::emit_rail_draw`'s delegating
// signature, unconditional whenever `mesh` is on) and `power` (`McuPlatform::emit_rail_draw`'s real
// override) — gated on `any(...)` so enabling either alone still compiles, and enabling both
// doesn't produce a duplicate-import error.
#[cfg(any(feature = "mesh", feature = "power"))]
pub use lib_mapping::schema::DeviceRailReading;
// `power` feature only (this crate's own Cargo.toml comment: enables `lib-io/ina3221` too).
// `impl_mcu_platform.rs`'s `emit_rail_draw` override + `publish_rail_draw`, and
// `impl_ina3221_rail_monitor.rs`'s `RailMonitor` impl for the driver — deliberately NOT
// `lib_mapping`: this diagnostic leg must never build a TMS struct (see the correction this
// module's doc history records).
#[cfg(feature = "power")]
pub use embedded_hal_async::i2c::I2c;
#[cfg(feature = "power")]
pub use lib_io::ina3221::Ina3221;
#[cfg(feature = "power")]
pub use lib_mapping::schema::DeviceRail;
#[cfg(feature = "power")]
pub use spec_app::RailMonitor;
