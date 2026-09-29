//! `embassy::udp` — the owned UDP/IPv6 transport seam, shaped to embassy-net's UDP socket model.
//!
//! A sibling of the [`crate::embassy`] spike's [`adapter`](crate::embassy::adapter) /
//! [`config`](crate::embassy::config) / [`pump`](crate::embassy::pump) /
//! [`host`](crate::embassy::host) modules, and gated under the same feature scheme:
//!
//! * feature `embassy-udp` — the portable, `no_std`, alloc-free seam: [`UdpEndpoint`] (the async
//!   send/recv/local-addr trait), [`UdpError`], and [`UdpBindSpec`]. This is the contract a future
//!   MCU impl (#328) satisfies NATIVELY over `embassy_net::udp::UdpSocket` +
//!   `embassy_net::Stack::join_multicast_group` — see [`UdpEndpoint`]'s doc for the per-method
//!   citations against embassy-net 0.6.0 (the version locked in the workspace).
//! * feature `embassy-udp-host` — [`HostUdpSocket`], the host-runner substrate (std + socket2 over
//!   the kernel stack). It lives here, in the embassy runner layer, because embassy/host's
//!   `build_stack` has no UDP-over-real-LAN substrate today (IPv4/TCP-only over an isolated tuntap
//!   device); per the Platform & Deployment doctrine the Pi's real substrate is the kernel stack.
//!   This is a runner-layer shim satisfying the embassy-shaped seam — NOT a parallel top-level
//!   transport. `socket2` is confined to this shim.
//!
//! `embassy-udp` deliberately pulls NONE of the embassy-net dependency stack — the seam is pure
//! `core::net`, so the Pi binary (lib-dds → `embassy-udp-host`) never compiles smoltcp/embassy-net
//! just to speak the contract. The full MCU impl, when #328 lands, adds an `embassy_net`-backed
//! `UdpEndpoint` under a `no_std` feature that turns on embassy-net's `udp`/`proto-ipv6`/`multicast`.

pub mod types;

mod impl_udp_bind_spec;
mod impl_udp_error;

#[cfg(feature = "embassy-udp-host")]
mod impl_host_udp_socket;
#[cfg(feature = "embassy-udp-host")]
mod prelude;

pub use types::{UdpBindSpec, UdpEndpoint, UdpError};

#[cfg(feature = "embassy-udp-host")]
pub use types::HostUdpSocket;
