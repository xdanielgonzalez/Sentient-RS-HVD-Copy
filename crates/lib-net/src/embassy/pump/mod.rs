//! The broker pump: drives [`crate::link::BrokerCore`] over any `embedded_io_async`-shaped transport.
//!
//! Two layers, deliberately separated:
//! * [`drain_and_write`] (deliverable 1) — generic purely over [`embedded_io_async::Write`]. Loops
//!   `BrokerCore::drain_outgoing` -> `write_all` until nothing more is staged, never calling
//!   `drain_outgoing` again until the previous packet is FULLY written (the frozen `BrokerCore`
//!   contract's own requirement — see that crate's `mqtt_server` module docs). `no_std`, no `alloc`,
//!   unaware of embassy-net/TcpSocket — unit-testable against an in-memory mock writer (see this
//!   module's own `#[cfg(test)]` in `drain_write.rs`), and reusable byte-for-byte against a real MCU
//!   [`embassy_net::tcp::TcpSocket`] (which implements `embedded_io_async::Write`).
//! * [`broker_slot_cycle`] (deliverable 2) — one client slot's accept -> recv/feed/drain -> close
//!   lifecycle, generic over any `Read + Write` stream plus caller-supplied `accept`/`close`
//!   closures (accept/close aren't `embedded_io_async` operations). Reusable library logic, not
//!   test-only.
//!
//! ## Interleaving across `MAX_CLIENTS` slots
//!
//! [`broker_slot_cycle`] covers exactly ONE slot's connection lifecycle. Each `TcpSocket`
//! accept+read+write naturally suspends independently of every other socket, so the cleanest way to
//! serve `MAX_CLIENTS` slots concurrently is one `broker_slot_cycle` future per slot, each looping
//! (`loop { broker_slot_cycle(...).await }` to keep re-listening after a peer disconnects) and
//! either spawned as its own `#[embassy_executor::task]` (N tasks, real MCU/host-executor
//! deployment) or raced via `embassy_futures::join`/`select` (a single-future test harness without
//! a multi-task executor). This crate does NOT itself pick between those two — `#[embassy_executor
//! ::task]` functions must be concrete/non-generic (same constraint M1's `net_task`/`ingress_task`/
//! `egress_task` document), so the concrete task wrapper closing over one specific slot's `&mut
//! BrokerCore`/`TcpSocket` necessarily lives at the call site (deliverable 3's test, and eventually
//! the real Zephyr wiring — out of scope here), not in this library. What IS shared/reusable is the
//! per-slot loop body itself: [`broker_slot_cycle`].
//!
//! Since each slot owns an independent `BrokerCore` slot index but all slots share the SAME
//! `&mut BrokerCore<MAX_CLIENTS>`, a caller spawning N concurrent `broker_slot_cycle` futures needs
//! either N `BrokerCore`s behind some sharing strategy, or to serialize access (e.g. one
//! single-threaded executor driving all N slot futures cooperatively, so `&mut BrokerCore` is never
//! actually aliased — each `.await` point is a clean handoff, not concurrent access). The
//! deliverable-3 test takes the single-threaded/cooperative shape (all slot cycles polled by the
//! same executor thread), matching how `BrokerCore` itself is `!Sync`-shaped (no internal locking)
//! and expects a single owner.

mod async_broker_pump;
mod broker_pump;
mod drain_write;
mod impl_latest_store;
mod types; // Doc-only: bin wiring guide for the device-hosted broker.

pub use async_broker_pump::async_broker_pump;
pub use broker_pump::broker_slot_cycle;
pub use drain_write::drain_and_write;
pub use types::{BrokerEvent, DrainWriteError, LatestStore};
