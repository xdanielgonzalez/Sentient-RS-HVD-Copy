//! `test-kit` — the single, consolidated test-support crate for sentient-rs (#99).
//!
//! Merges three formerly-separate test-support homes so test infrastructure lives in one
//! findable place:
//!
//! * [`bridge`] — `LoopbackBridge`/`BridgeEnd` (formerly the standalone `sentient-net-testkit`
//!   crate): a two-stack in-memory [`lib_net::channel::WireFrame`] bridge for owned-net-stack
//!   loopback tests.
//! * [`seplos`] — `SeplosSim` (formerly the standalone `seplos-sim` crate): a pure,
//!   transport-agnostic Seplos v2 BMS responder for e2e test harnesses.
//! * [`mqtt`] — the embedded-broker test helpers + `FakeTokenVerifier` (formerly
//!   `lib-mqtt/tests/support/mod.rs`).
//! * [`tls`] — ephemeral TLS bring-up/test identity generation (`rcgen`), replacing a
//!   formerly-committed private key — see that module's docs.
//!
//! This is a first-party, `publish = false` crate that exists purely so integration tests across
//! the workspace (`lib-link`, `lib-mqtt`, `test-e2e`) can depend on it as a
//! **dev-dependency** — it is never a runtime dependency of any shipping binary and never touches
//! the no_std MCU build.
#![forbid(unsafe_code)]

pub mod bridge;
#[cfg(feature = "dds-probe")]
pub mod dds;
pub mod mqtt;
pub mod seplos;
pub mod tls;
pub mod voltronic;

pub use bridge::{BridgeEnd, LoopbackBridge};
#[cfg(feature = "dds-probe")]
pub use dds::start_dds_probe;
pub use mqtt::{
    invalid_token, reserve_ephemeral_port, start_authenticated_broker, start_broker, valid_token,
    FakeTokenVerifier, AUTH_AUDIENCE, AUTH_ISSUER, AUTH_TRUSTED_KEY,
};
pub use seplos::{
    parse_battery_state_spec, parse_drift_spec, BatteryState, BatteryStateSpec, Behavior,
    DriftSpec, FaultFlags, SeplosSim, SimReply, MAX_CELLS, MAX_FRAME, MAX_SERIAL, MAX_TEMPS,
};
pub use tls::{generate_bringup_identity, GeneratedTlsIdentity};
pub use voltronic::VoltronicSim;
