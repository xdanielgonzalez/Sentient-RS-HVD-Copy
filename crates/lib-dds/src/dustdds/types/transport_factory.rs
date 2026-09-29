use core::net::Ipv6Addr;

/// A custom UDP/IPv6 `dust_dds` `TransportParticipantFactory`.
///
/// dust_dds 0.15.0's built-in transport is IPv4-only (`rtps_udp_transport` filters `Addr::V4`,
/// `todo!()`s on `LOCATOR_KIND_UDP_V6`), but the transport interface is a public generic seam and
/// `Locator` already carries `LOCATOR_KIND_UDP_V6`. This factory binds IPv6 sockets, joins the SPDP
/// `ff02::` multicast group on the configured interface, and advertises IPv6 unicast locators. Impl
/// in `impl_transport_factory.rs`.
pub struct Ipv6TransportFactory {
    /// Interface scope / if-index (from `/sys/class/net/<iface>/ifindex`) for link-local multicast.
    pub(crate) scope: u32,
    /// SPDP discovery multicast group (`ff02::` link-local).
    pub(crate) group: Ipv6Addr,
    /// Address advertised in the unicast locator lists (`::1` same-host, or the iface address).
    pub(crate) unicast_addr: Ipv6Addr,
    /// SPDP multicast port (`7400 + 250 * domain_id`).
    pub(crate) spdp_port: u16,
    /// RTPS fragment size advertised to the core.
    pub(crate) fragment_size: usize,
}
