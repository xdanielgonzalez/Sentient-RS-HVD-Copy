//! [`UdpBindSpec`] — the per-socket bind + multicast-setup parameters for a host UDP endpoint.

use core::net::{Ipv6Addr, SocketAddrV6};

/// Everything needed to bind ONE UDP/IPv6 socket and apply its one-time multicast setup, folded into
/// a single value so the [`super::udp_endpoint::UdpEndpoint`] trait stays a pure send/recv runtime
/// surface (see that trait's doc for why `bind`/JOIN are setup, not trait methods).
///
/// The three constructors map 1:1 onto the sockets an RTPS/UDP participant needs:
/// [`unicast`](Self::unicast) (user + metatraffic unicast, ephemeral port), [`multicast_listener`]
/// (the shared SPDP port: `SO_REUSEADDR` + group JOIN + loop), and [`multicast_sender`] (egress:
/// `IPV6_MULTICAST_IF` + loop). Impls in `impl_udp_bind_spec.rs`.
///
/// [`multicast_listener`]: Self::multicast_listener
/// [`multicast_sender`]: Self::multicast_sender
#[derive(Debug, Clone, Copy)]
pub struct UdpBindSpec {
    /// Local address to bind. Port `0` = OS-assigned ephemeral, mirroring embassy-net's
    /// `UdpSocket::bind` port-0 dynamic-allocation semantics (embassy-net 0.6.0 `udp.rs:78`).
    pub bind: SocketAddrV6,
    /// `IPV6_V6ONLY`. Always `true` here — this transport is IPv6-only.
    pub only_v6: bool,
    /// `SO_REUSEADDR` (+ best-effort `SO_REUSEPORT`) — required so several participants can share the
    /// fixed SPDP multicast port on one host.
    pub reuse: bool,
    /// Join this multicast group after bind (the SPDP metatraffic socket). On the MCU (#328) this
    /// maps to `embassy_net::Stack::join_multicast_group` (embassy-net 0.6.0 `lib.rs:629`).
    pub join_multicast: Option<Ipv6Addr>,
    /// Set `IPV6_MULTICAST_IF` to this interface index (the egress socket, so multicast leaves the
    /// right link).
    pub multicast_if: Option<u32>,
    /// `IPV6_MULTICAST_LOOP` — deliver our own multicast back to same-host listeners.
    pub multicast_loop: bool,
    /// Interface scope / if-index for the multicast join.
    pub scope: u32,
}
