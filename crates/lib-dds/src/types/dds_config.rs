use core::net::Ipv6Addr;

/// Runtime configuration for the DDS stack + its custom UDP/IPv6 transport.
///
/// Construct via [`DdsConfig::domain0`] (the common case) or a struct literal. Impl in
/// `impl_dds_config.rs`.
#[derive(Debug, Clone)]
pub struct DdsConfig {
    /// DDS domain (MIL-STD-3071 mandates domain 0).
    pub domain_id: u16,
    /// Network interface name carrying the SPDP multicast (e.g. `"eth0"`). The transport reads its
    /// scope/if-index from `/sys/class/net/<iface>/ifindex`.
    pub iface: String,
    /// SPDP discovery multicast group (link-local `ff02::` scope). RTPS SPDP port for the domain is
    /// `7400 + 250 * domain_id`.
    pub mcast_group: Ipv6Addr,
    /// Address advertised in the unicast locator lists. `None` = `::1` loopback (same-host). Set to
    /// the interface's routable/link-local address for cross-host or CycloneDDS interop.
    pub unicast_addr: Option<Ipv6Addr>,
    /// Bounded capacity of the outbound [`super::DdsOutbound`] queue ([`super::DdsHandle`] → stack).
    pub queue_capacity: usize,
}
