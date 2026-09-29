//! `lib-net`'s **embassy seam** — the owned `embassy-net`/smoltcp stack composed with our existing
//! `NetLink`/`WireFrame` abstractions, host-testable and building for the MCU target before any
//! emulator run.
//!
//! **Status (EPIC #350, story S8 #359): bumped to the new embassy generation and now a live MCU
//! consumer target.** The stack was moved off the original embassy-net 0.6.0 spike pins onto the
//! same generation `bin/mcu-rp2350` runs — embassy-net **0.9.1** / embassy-time **0.5** / embassy-sync
//! **0.8** / embassy-executor **0.10** (host) / embedded-io-async **0.7** / heapless **0.9** — so
//! `bin/mcu-rp2350` (embassy-rp 0.10) consumes THIS seam directly (`adapter`/`config`/`pump`/`wiznet`)
//! instead of forking bespoke W5500/broker glue into the bin. The owned embassy-net/smoltcp path is
//! MCU-only (Pi ships Yocto Linux since the 2026-07-12 pivot, story #159). It builds via `just
//! build-no-std` (part of `no_std_net`) and is excluded from the `just coverage` line-coverage gate,
//! since its real coverage comes from the `embassy-host`-feature integration tests
//! (`tests/host_bring_up.rs`, `tests/broker_pump.rs`), which `cargo tarpaulin` can't build. See this
//! crate's `README.md` for the fuller rationale and how it relates to `lib-link`'s `NetStack`.
//!
//! The `channel` module (`crate::channel::channel`, a sibling of this `embassy` module, NOT a
//! submodule of it) is a DIFFERENT story: its `embassy-sync` `Sender`/`Receiver` impls (feature
//! `channel-embassy`) now have a live MCU consumer — `bin/mcu-rp2350`'s `bin-mcu-platform` (the
//! Embassy-runtime pivot, EPIC #350) — so "deliberately retained, consumed by no crate" applies
//! ONLY to this `embassy` module's embassy-net spike, not to the channel layer as a whole.
//!
//! * [`adapter`] — bridges [`crate::link::NetLink`] to `embassy-net-driver-channel`'s `Runner`
//!   (`link.recv()` -> the channel's RX queue, the channel's TX queue -> `link.send()`), so the
//!   SAME driver (Zephyr NIC / W5500 / [`crate::link::LoopbackLink`] / host
//!   [`crate::link::host::HostLink`]) can sit under `embassy_net::Stack` instead of (or alongside)
//!   the bespoke [`crate::link::stack::NetStack`].
//! * [`config`] — the static IPv4 [`config::StackConfig`] the M1 bring-up builds a stack from.
//! * [`pump`] — **Milestone 2**: the device-hosted MQTT broker pump ([`crate::link::BrokerCore`])
//!   driven over any `embedded_io_async`-shaped transport (a real `embassy_net::tcp::TcpSocket` or a
//!   host mock) — see that module's docs for the drain-and-write core / per-slot cycle split.
//! * `host` (feature `host`) — the host-only bring-up proof: an `embassy-executor` `arch-std`
//!   executor driving a real `embassy_net::Stack` + `TcpSocket` over the adapter. Never compiled
//!   for the no_std MCU target.
//!
//! `no_std`-by-default note: this crate's core (`adapter`/`config`/`pump`) pulls in nothing but
//! `core` + `heapless` + `embassy-net`/`embassy-net-driver-channel`/`embedded-io-async`/`lib-link`,
//! all no_std/no-alloc, matching [`crate::link`]'s own no_std-by-default posture. The `host` feature
//! (embassy-executor's `arch-std`, `embassy-net-tuntap`, `log`) is std-only and gated so `cargo
//! build --no-default-features --target thumbv7em-none-eabihf` never touches it.

#[cfg(feature = "embassy")]
pub mod adapter;
#[cfg(feature = "embassy")]
pub mod config;
#[cfg(feature = "embassy")]
pub mod pump;

#[cfg(feature = "embassy-host")]
pub mod host;

// The W5500 Ethernet device leg (feature `embassy-wiznet`): the on-board WIZnet W5500 as an
// embassy-net `Device`/`Runner` the Stack path above consumes. Generic over the caller's
// embedded-hal-async SPI/GPIO — no embassy-rp types here (see the module doc).
#[cfg(feature = "embassy-wiznet")]
pub mod wiznet;

// The owned UDP/IPv6 transport seam (feature `embassy-udp`; host shim under `embassy-udp-host`). A
// sibling of the M1/M2 spike modules above — the embassy-net-shaped async UDP contract lib-dds's
// RTPS transport programs against. Compiles WITHOUT the embassy-net dep stack (pure `core::net`), so
// `embassy-udp` can be enabled (e.g. by the std/host RTPS path) without turning on `embassy` itself.
#[cfg(feature = "embassy-udp")]
pub mod udp;

#[cfg(feature = "embassy")]
pub use adapter::{pump_channel_to_link, pump_link_to_channel};
#[cfg(feature = "embassy")]
pub use config::StackConfig;
#[cfg(feature = "embassy")]
pub use pump::{
    async_broker_pump, broker_slot_cycle, drain_and_write, BrokerEvent, DrainWriteError,
    LatestStore,
};

#[cfg(feature = "embassy-wiznet")]
pub use wiznet::{
    build_w5500_stack, link_local_v6, wait_for_config, wait_for_config_v6, W5500Parts,
};

#[cfg(feature = "embassy-udp-host")]
pub use udp::HostUdpSocket;
#[cfg(feature = "embassy-udp")]
pub use udp::{UdpBindSpec, UdpEndpoint, UdpError};
