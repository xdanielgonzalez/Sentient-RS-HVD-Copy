//! Type definitions for the Embassy MCU platform.

mod channel_publisher;
// WP4 correction: the local newtype letting this crate implement `spec_app::RailMonitor` for
// `lib_io::ina3221::Ina3221` (orphan-rule workaround — see this type's own doc comment). Gated
// behind the `power` feature, same reasoning as `mesh` below.
#[cfg(feature = "power")]
mod ina3221_rail_monitor;
mod mcu_clock;
mod mcu_platform;
// WP3 (delight-features): the Meshtastic mesh-radio forwarding decorator + its channel-message
// type. Gated behind the `mesh` feature so the default (non-mesh) build of this crate — and every
// consumer that doesn't opt in — is unchanged.
#[cfg(feature = "mesh")]
mod mesh_platform;
#[cfg(feature = "mesh")]
mod mesh_unit;

pub use channel_publisher::*;
#[cfg(feature = "power")]
pub use ina3221_rail_monitor::*;
pub use mcu_clock::*;
pub use mcu_platform::*;
#[cfg(feature = "mesh")]
pub use mesh_platform::*;
#[cfg(feature = "mesh")]
pub use mesh_unit::*;
