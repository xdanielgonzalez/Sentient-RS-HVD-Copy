//! `impl` blocks for [`super::StackConfig`].

use super::types::StackConfig;
use embassy_net::{Ipv4Address, Ipv4Cidr};

impl StackConfig {
    /// The M1 bring-up default: `192.0.2.1/24` (RFC 5737 TEST-NET-1) on `mac`.
    pub fn test_net_1(mac: [u8; 6]) -> Self {
        StackConfig {
            cidr: Ipv4Cidr::new(Ipv4Address::new(192, 0, 2, 1), 24),
            mac,
        }
    }

    /// The embassy-net `Config` this builds (static IPv4, no gateway/DNS — the M1 proof is a
    /// direct peer-to-peer TCP connection on one link, not routed).
    pub fn embassy_config(&self) -> embassy_net::Config {
        embassy_net::Config::ipv4_static(embassy_net::StaticConfigV4 {
            address: self.cidr,
            gateway: None,
            // embassy-net 0.9.1's `dns_servers` is `heapless::Vec` from heapless 0.9 (the workspace
            // pin) — constructed directly now that the former heapless-0.8 shim is gone.
            dns_servers: heapless::Vec::new(),
        })
    }
}
