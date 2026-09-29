//! `SeplosResponder` — a pure, transport-agnostic Seplos v2 BMS responder: request-frame bytes
//! in, response-frame bytes out. Telemetry (`0x42`) only, DIP address 0, always-success
//! (RTN=0x00) — the phase-1 HIL bench scope (see `hil/README.md`). Fault injection (`0x44`),
//! serial-number (`0xA4`), and the `Behavior` fault-injection matrix are explicitly NOT ported
//! here; add them as new files (`impl_fault.rs`, etc.) when phase 2 needs them, not by rewriting
//! this module.
//!
//! **Deliberate fork, not a dependency**, of
//! `crates/test-kit/src/seplos/{impl_seplos_sim_encode,impl_seplos_sim_parse,
//! impl_seplos_sim_respond}.rs` (the reference `SeplosSim` driving the pty-backed e2e suite).
//! `test-kit` is `std`-only (tokio, rcgen, lib-mqtt with default features) so it can't be a
//! dependency of this `no_std` crate — this module is a byte-faithful trim of that reference,
//! restricted to the Normal-behavior/Telemetry-only subset. See `docs/seplos-sim.md` for known
//! protocol-fidelity gaps in the original (not relevant to the telemetry-only path this module
//! covers).
//!
//! `checksum.rs` VENDORS `lib_parse::seplos::checksum` verbatim rather than depending on
//! `lib-parse` — see this crate's `Cargo.toml` header comment for why (a dead, unconditional
//! `bytes` dependency in `lib-parse` fails to build for `thumbv6m-none-eabi`).

pub(crate) mod checksum;
mod impl_battery_state;
mod impl_drift_spec;
mod impl_encode;
mod impl_parse;
mod impl_respond;
mod impl_responder;
mod impl_sim_reply;
mod prelude;
mod types;

pub use types::*;

/// Max cells represented in one synthesized reading. Mirrors `lib_parse::canonical::MAX_CELLS`.
pub const MAX_CELLS: usize = 16;
/// Max temperature sensors in one synthesized reading. Mirrors `lib_parse::canonical::MAX_TEMPS`.
pub const MAX_TEMPS: usize = 8;
/// Max on-wire response frame length (SOI + header + hex INFO + CHKSUM + EOI). A 14-cell/6-temp
/// telemetry reply is 160 bytes on the wire (verified in `tests/responder_host.rs`); 256 gives
/// headroom to the full 16-cell/8-temp shape without ever needing a resize.
pub const MAX_FRAME: usize = 256;
