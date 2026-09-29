//! Broker-lifecycle error + an in-memory test [`MockPublisher`].
//!
//! The publish *contract* (the `Publisher` trait, topic builder, `publish_tms`) is the no_std
//! `lib_mapping::sink` — shared by this crate's embedded broker (std) and the Zephyr MQTT
//! client (no_std). This module only holds the std-side broker-lifecycle error and a test mock.

mod prelude;
mod types;
pub use types::*;

mod impl_mock_publisher;
