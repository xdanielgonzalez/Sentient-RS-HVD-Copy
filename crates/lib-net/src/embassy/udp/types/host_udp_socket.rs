//! [`HostUdpSocket`] — the host-runner UDP substrate (feature `embassy-udp-host`).

use std::net::UdpSocket;
use std::sync::Arc;

/// A host (std/kernel-stack) UDP/IPv6 socket implementing the embassy-shaped
/// [`super::udp_endpoint::UdpEndpoint`] seam.
///
/// ## Why a std shim lives in the embassy runner layer
///
/// The owned embassy-net stack ([`crate::embassy`]) is the portability seam for MCU⇆RPi transport,
/// but its host bring-up ([`crate::embassy::host::build_stack`]) stands up an `embassy_net::Stack`
/// over `embassy-net-tuntap` with only `medium-ethernet` / `proto-ipv4` / `tcp` — no `udp` /
/// `proto-ipv6` / `multicast` sockets, and that tuntap device is isolated from the real LAN. So
/// embassy/host genuinely has **no** UDP-over-real-network substrate today. Per the Platform &
/// Deployment doctrine (CLAUDE.md: the Pi ships Yocto Linux and the owned embassy-net stack is
/// MCU-only), the Pi's real substrate is the kernel network stack. This type binds it via std +
/// socket2 and satisfies the SAME `UdpEndpoint` contract the MCU impl (#328) will satisfy natively
/// over `embassy_net::udp::UdpSocket` — a host-runner shim, never a parallel top-level transport.
///
/// The socket is `Arc`-wrapped so it is cheaply `Clone` (the send half moves into dust_dds's boxed
/// send future; a receive half moves into a dedicated receive task/thread). Impl (incl. the
/// [`bind`](HostUdpSocket::bind) constructor) in `impl_host_udp_socket.rs`.
#[derive(Clone)]
pub struct HostUdpSocket {
    pub(in crate::embassy::udp) sock: Arc<UdpSocket>,
}
