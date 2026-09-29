//! The app contract: the `Platform` port (ADR 0009) plus the shared vocabulary it speaks in.
//!
//! Ports & adapters (ADR 0009, `docs/decisions/0009-spec-app-ports-and-adapters.md`): `spec-app`
//! is the stable, abstract spine both `impl-app` (the policy — the poll → map → publish loop) and
//! every platform adapter (`bin-rpi-platform`, `bin-zephyr-platform`) depend *down* onto, so
//! neither the policy nor a detail ever depends on the other. It holds:
//!
//! - [`Platform`] — the seam the poll loop is written against (transport + publish sink + clock).
//! - [`RunConfig`] / [`Health`] — the platform-neutral run parameters and health counters the
//!   trait's methods are typed over.
//! - [`preflight`] — the startup preflight-check subsystem (adapter-shared contract vocabulary:
//!   both adapters and both bins implement/run it, but `impl_app::run` never touches it — moving
//!   it here means `bin-rpi-platform` needs zero `impl-app` dependency, normal or dev).
//!
//! Depends only on the trait/data crates its signatures name (`lib-io`'s `SerialTransport`,
//! `lib-mapping`'s `Publisher`/`ClockMonotonic`, `lib-parse`'s canonical model) — no policy, no
//! platform concretion. Unconditionally `no_std`: this contract must compile on the MCU target
//! with zero feature gymnastics.
//!
//! # Example
//!
//! Build the platform-neutral [`RunConfig`] a platform hands to the poll loop, then run a
//! startup preflight against it — the platform's context opts into a
//! [`capabilities`](preflight::capabilities) trait and the generic
//! [`ConfigValid`](preflight::checks::ConfigValid) check does the rest:
//!
//! ```rust
//! use lib_mapping::schema::Identity;
//! use spec_app::preflight::capabilities::ConfigCap;
//! use spec_app::preflight::checks::ConfigValid;
//! use spec_app::preflight::run_checks;
//! use spec_app::RunConfig;
//!
//! # fn block_on<F: core::future::Future>(fut: F) -> F::Output {
//! #     let mut fut = core::pin::pin!(fut);
//! #     let mut cx = core::task::Context::from_waker(core::task::Waker::noop());
//! #     loop {
//! #         if let core::task::Poll::Ready(out) = fut.as_mut().poll(&mut cx) {
//! #             return out;
//! #         }
//! #     }
//! # }
//! # fn main() -> Result<(), heapless::CapacityError> {
//! let cfg = RunConfig {
//!     device_id: Identity::try_from("METEOR-001")?,
//!     prefix: heapless::String::try_from("tms")?,
//!     dip_addresses: heapless::Vec::from_slice(&[0x00, 0x01])?,
//!     interval_ms: 2_000,
//!     fault_every: 10,
//!     max_backoff_ms: 30_000,
//! };
//!
//! struct Ctx {
//!     cfg: RunConfig,
//! }
//! impl ConfigCap for Ctx {
//!     fn config_check(&self) -> Result<(), &'static str> {
//!         if self.cfg.dip_addresses.is_empty() {
//!             return Err("no DIP addresses configured");
//!         }
//!         Ok(())
//!     }
//! }
//!
//! let ctx = Ctx { cfg };
//! let report = block_on(async { run_checks!(ctx => ConfigValid) });
//! assert_eq!(report.passed(), 1);
//! assert!(report.ok());
//! # Ok(())
//! # }
//! ```
#![no_std]
#![forbid(unsafe_code)]

mod impl_health;
mod types;

pub use types::*;

pub mod preflight;

/// Max DIP addresses polled per cycle (bounded storage).
pub const MAX_DIPS: usize = 16;

/// Consecutive failed cycles past which the loop reports itself unhealthy ([`Health::healthy`]).
/// Telemetry-success accounting only — the systemd watchdog feed is liveness-based, not gated on
/// this (see [`Platform::on_cycle`]'s doc comment / bug #172).
pub const UNHEALTHY_AFTER_CYCLES: u32 = 3;
