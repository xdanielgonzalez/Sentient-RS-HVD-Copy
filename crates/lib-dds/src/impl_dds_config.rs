use super::prelude::*;
use core::net::Ipv6Addr;

impl DdsConfig {
    /// Domain-0 config on `iface`, using the WP4 link-local SPDP group `ff02::1234` and `::1`
    /// (loopback) unicast advertisement (same-host default; override `unicast_addr` for interop).
    pub fn domain0(iface: impl Into<String>) -> Self {
        Self {
            domain_id: 0,
            iface: iface.into(),
            mcast_group: Ipv6Addr::new(0xff02, 0, 0, 0, 0, 0, 0, 0x1234),
            unicast_addr: None,
            queue_capacity: 256,
        }
    }

    /// The RTPS SPDP multicast port for this domain: `7400 + 250 * domain_id` (PB=7400, DG=250,
    /// multicast-meta offset d0=0 — matches CycloneDDS's defaults for interop).
    pub fn spdp_port(&self) -> u16 {
        7400u16.saturating_add(250u16.saturating_mul(self.domain_id))
    }
}
