//! Background tasks spawned alongside the primary poll loop, closing the TMS behavioral-conformance
//! gaps (see `docs/tms-compliance.md` §9 and `docs/gap-analysis-meteor-parity.md`):
//!
//! - [`heartbeat`] — the TMS-required 1 s Heartbeat + retained DeviceInfo (per identity, MQTT).
//! - [`reply_listener`] — the Request/Reply control path within the 0.1 s budget (per identity,
//!   MQTT); [`request_reply`] holds the transport-neutral request→`Reply` core it shares with the
//!   DDS listener (#324, WP6).
//! - [`state_publisher`] — the four Required STOR state topics (`EnergyStartStopState`,
//!   `PowerPortState`, `DcLoadSharingState`, `ActiveMicrogridControllerState`) — #263, per identity
//!   (MQTT).
//! - [`mc_select`] — the MC-selection state machine (A.7.5) — #264, ONE shared task feeding every
//!   identity's `state_publisher` the selected MC id via a `watch`.
//! - [`dds`] — the DDS/RTPS transport counterparts of `heartbeat`/`state_publisher`/
//!   `reply_listener` (#324, WP6), spawned only when `[dds]` is configured (`runtime::supervise`);
//!   MQTT stays always-on regardless.
//!
//! Per-identity tasks are spawned by `crate::runtime::supervise` ONCE PER derived identity (D13 —
//! one identity per bus), with injected params (identity + broker host:port + product info; no
//! `Config` reload). Each publishes/listens over its own loopback `rumqttc` client with an
//! identity-suffixed client-id. [`util`] holds the shared publish helpers; [`types`] +
//! [`impl_mc_selector`] hold the pure, host-tested MC-selection logic.

mod impl_mc_selector;
mod types;

pub mod dds;
pub mod heartbeat;
pub mod mc_select;
pub mod reply_listener;
pub mod request_reply;
pub mod state_publisher;
pub mod util;
