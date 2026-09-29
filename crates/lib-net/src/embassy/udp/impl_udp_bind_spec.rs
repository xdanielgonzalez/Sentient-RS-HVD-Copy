//! Constructors for [`UdpBindSpec`] — one per RTPS/UDP socket role. Portable (`core::net`); imports
//! the type directly (no std prelude in the no_std seam layer).

use core::net::{Ipv6Addr, SocketAddrV6};

use super::types::UdpBindSpec;

impl UdpBindSpec {
    /// A unicast socket on an OS-assigned ephemeral port (the user-data + metatraffic-unicast
    /// sockets). No multicast setup; `scope` is retained for link-local sends.
    pub fn unicast(scope: u32) -> Self {
        UdpBindSpec {
            bind: SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0),
            only_v6: true,
            reuse: false,
            join_multicast: None,
            multicast_if: None,
            multicast_loop: false,
            scope,
        }
    }

    /// The shared SPDP metatraffic-multicast listener: bind the fixed SPDP `port` with
    /// `SO_REUSEADDR`, join `group` on `scope`, and loop our own multicast back to same-host peers.
    pub fn multicast_listener(group: Ipv6Addr, port: u16, scope: u32) -> Self {
        UdpBindSpec {
            bind: SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, port, 0, 0),
            only_v6: true,
            reuse: true,
            join_multicast: Some(group),
            multicast_if: None,
            multicast_loop: true,
            scope,
        }
    }

    /// The multicast egress socket: an ephemeral port with `IPV6_MULTICAST_IF` pinned to `scope` so
    /// outbound multicast leaves the right interface, loop on.
    pub fn multicast_sender(scope: u32) -> Self {
        UdpBindSpec {
            bind: SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0),
            only_v6: true,
            reuse: false,
            join_multicast: None,
            multicast_if: Some(scope),
            multicast_loop: true,
            scope,
        }
    }
}
