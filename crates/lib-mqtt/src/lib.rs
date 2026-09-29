//! `lib-mqtt` — the device's local MQTT surface.
//!
//! `bin-rpi` embeds an **rumqttd MQTT server** (the per-device local MQTT server) and publishes
//! mapped TMS payloads onto it via the in-process [`broker`] Link. An external **mosquitto** bridge
//! connects to that broker, subscribes to the TMS topics, and relays them to the private-cloud
//! endpoint (and Greengrass) — mosquitto reads *from* us; the device never pushes to the cloud.
//!
//! * [`publisher`] — the [`Publisher`] sink trait (re-exported from `lib_mapping::sink`) +
//!   topic/JSON helpers + a test mock.
//! * [`broker`] — the embedded rumqttd server + a `Publisher` backed by its `LinkTx`.
#![forbid(unsafe_code)]

pub mod broker;
pub mod publisher;

pub use publisher::{MockPublisher, NetError};
// The publish contract itself is the shared no_std `lib_mapping::sink` (so a Zephyr MQTT
// client can implement the same trait) — re-exported here for RPi-side ergonomics.
pub use lib_mapping::sink::{publish_tms, topic_into, PublishError, Publisher, DEFAULT_PREFIX};
