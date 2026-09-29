//! `lib-net` — the device's consolidated network layer (#287).
//!
//! One dual-target crate folding the former `lib-mesh`, `lib-locate`, `lib-link`, `lib-channel`,
//! and `lib-net-embassy` crates into feature-gated modules, following the codebase's existing
//! dual-target pattern (see `impl-app` / `lib-io` / `lib-parse`):
//!
//! * **host / std publish targets (RPi deployment)** — [`mesh`] (Meshtastic mesh-radio publish
//!   target, feature `mesh`) and [`locate`] (multi-tier location service, feature `locate`); the
//!   `host` umbrella feature enables both. std-only; never enters the no_std MCU build.
//! * **owned-net MCU core (no_std)** — [`channel`] (the always-compiled `Sender`/`Receiver` +
//!   boundary-message contract; strict postcard/serde codec behind feature `packed`), `link`
//!   (the L2 driver + smoltcp L3/L4 stack + owned MQTT broker/client/session, feature `link`; the
//!   Linux pnet/AF_PACKET driver behind `link-host`, the TLS record layer behind `tls`), and
//!   `embassy` (the embassy-net owned-stack spike, feature `embassy`; host bring-up behind
//!   `embassy-host`).
//!
//! The MCU build (`cargo build -p lib-net --no-default-features --features link,embassy --target
//! thumbv7em-none-eabihf`) stays `no_std`: every std module and every std driver seam is
//! feature-gated so neither target absorbs the other's dependencies.
#![cfg_attr(not(any(test, feature = "std")), no_std)]
#![forbid(unsafe_code)]

// Always-compiled no_std base: the inter-stage channel contract + boundary messages (heapless-only).
// `link` and `embassy` build on it; test-kit consumes its `WireFrame` directly.
pub mod channel;

// The owned L2/L3/L4 stack (no_std core; std host driver + TLS behind `link-host` / `tls`).
#[cfg(feature = "link")]
pub mod link;

// The embassy-net owned-stack spike (no_std MCU; host bring-up behind `embassy-host`) + the owned
// UDP/IPv6 transport seam (`embassy::udp`, feature `embassy-udp`; host shim `embassy-udp-host`). The
// seam sub-module is pure `core::net`, so `embassy-udp` alone compiles this module without the
// embassy-net dependency stack.
#[cfg(any(feature = "embassy", feature = "embassy-udp"))]
pub mod embassy;

// std/host publish targets (RPi deployment).
#[cfg(feature = "locate")]
pub mod locate;

#[cfg(feature = "mesh")]
pub mod mesh;
