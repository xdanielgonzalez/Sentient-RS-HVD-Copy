use core::future::Future;
use core::net::{Ipv6Addr, SocketAddrV6};
use core::pin::Pin;
use std::sync::Arc;

use dust_dds::transport::interface::WriteMessage;
use dust_dds::transport::types::{Locator, LOCATOR_KIND_UDP_V6};
use lib_net::embassy::udp::UdpEndpoint;

use super::prelude::*;

impl WriteMessage for Ipv6MessageWriter {
    /// Send an already-framed RTPS datagram to each UDP/IPv6 destination locator. Link-local /
    /// multicast destinations are scoped to the transport's interface; loopback/global are not.
    ///
    /// The destinations are resolved while `locators` is still borrowed, then an owned copy of the
    /// datagram plus a cheap clone of the send endpoint move into the boxed future dust_dds awaits —
    /// the future is `'static`, so the borrowed `buf` cannot cross the await. Sends go through the
    /// owned net layer's async [`UdpEndpoint::send_to`] (the embassy-native send path: an immediate
    /// kernel syscall on the host, a real suspend on the MCU); dust_dds drives the returned future
    /// on its own executor.
    fn write_message(
        &self,
        buf: &[u8],
        locators: &[Locator],
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let mut dests: Vec<SocketAddrV6> = Vec::new();
        for loc in locators {
            if loc.kind() != LOCATOR_KIND_UDP_V6 {
                continue; // this transport speaks IPv6 only
            }
            let addr = Ipv6Addr::from(loc.address());
            let port = loc.port() as u16;
            let scope = if addr.is_multicast() || is_link_local(&addr) {
                self.scope
            } else {
                0
            };
            dests.push(SocketAddrV6::new(addr, port, 0, scope));
        }
        let send_sock = self.send_sock.clone();
        let data: Arc<[u8]> = Arc::from(buf);
        Box::pin(async move {
            for dst in dests {
                let _ = send_sock.send_to(&data, dst).await;
            }
        })
    }
}

/// `fe80::/10` — link-local unicast, which needs an interface scope to route.
fn is_link_local(a: &Ipv6Addr) -> bool {
    (a.segments()[0] & 0xffc0) == 0xfe80
}
