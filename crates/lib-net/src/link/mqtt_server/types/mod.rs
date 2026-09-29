mod active_topic;
mod broker_core;
mod broker_error;
mod client_slot;
mod control_frame;
mod pending_publish;

// `BrokerCore`/`BrokerError` are this crate's public API (re-exported from `crate::link::mqtt_server`,
// and from `lib.rs`); the rest are internal-only broker bookkeeping types, module-private in the
// original single-file `mqtt_server.rs` — `pub(in crate::link::mqtt_server)` preserves that scope now
// that they live in a `types` sub-module rather than inline with their impls.
pub use broker_core::*;
pub use broker_error::*;

pub(in crate::link::mqtt_server) use active_topic::*;
pub(in crate::link::mqtt_server) use client_slot::*;
pub(in crate::link::mqtt_server) use control_frame::*;
pub(in crate::link::mqtt_server) use pending_publish::*;
