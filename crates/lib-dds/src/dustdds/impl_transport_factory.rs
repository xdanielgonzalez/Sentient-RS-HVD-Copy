use core::future::Future;
use core::net::Ipv6Addr;
use std::sync::Arc;

use dust_dds::dcps::channels::mpsc::MpscSender;
use dust_dds::std_runtime::executor::block_on;
use dust_dds::transport::interface::{RtpsTransportParticipant, TransportParticipantFactory};
use dust_dds::transport::types::{Locator, LOCATOR_KIND_UDP_V6};

use lib_net::embassy::udp::{HostUdpSocket, UdpBindSpec, UdpEndpoint};

use super::prelude::*;

/// One RTPS datagram never exceeds 64 KiB; the receive buffer is sized to the max UDP payload.
const MAX_DATAGRAM: usize = 65_507;

impl TransportParticipantFactory for Ipv6TransportFactory {
    // The trait's `create_participant` is INFALLIBLE by signature (`-> RtpsTransportParticipant`), so
    // one-shot socket/multicast setup cannot propagate a `Result`. A bind/join failure here means a
    // misconfigured interface at STACK STARTUP (the common case — a bad iface name — is already a
    // clean `DdsError` via `read_ifindex`); failing fast + loud at boot is correct (a process must
    // not come up with a silently-broken transport). This mirrors dust_dds's own built-in UDP
    // transport, which `unwrap()`s the same setup. Startup only — never the hot path.
    #[allow(clippy::expect_used)]
    fn create_participant(
        &self,
        _domain_id: i32,
        data_channel_sender: MpscSender<Arc<[u8]>>,
    ) -> impl Future<Output = RtpsTransportParticipant> + Send {
        let scope = self.scope;
        let group = self.group;
        let uni = self.unicast_addr;
        let sport = self.spdp_port;
        let fragment_size = self.fragment_size;
        async move {
            // Bind through the owned net layer's UDP/IPv6 seam (`lib_net::embassy::udp`) — this crate
            // holds no raw sockets. Each `UdpBindSpec` constructor folds one socket's bind +
            // multicast setup (see that module).

            // Two unicast sockets (user default + metatraffic), ephemeral ports.
            let unicast =
                HostUdpSocket::bind(&UdpBindSpec::unicast(scope)).expect("bind default unicast");
            let default_unicast_port = local_port(&unicast);
            let meta_unicast = HostUdpSocket::bind(&UdpBindSpec::unicast(scope))
                .expect("bind metatraffic unicast");
            let meta_unicast_port = local_port(&meta_unicast);

            // Metatraffic multicast socket: bind the SPDP port (reuse), join the group on the iface,
            // loop on.
            let mcast = HostUdpSocket::bind(&UdpBindSpec::multicast_listener(group, sport, scope))
                .expect("bind metatraffic multicast");

            // Egress socket: multicast interface = scope, loop on.
            let send_sock =
                HostUdpSocket::bind(&UdpBindSpec::multicast_sender(scope)).expect("bind egress");

            spawn_receiver(unicast, data_channel_sender.clone(), "uni");
            spawn_receiver(meta_unicast, data_channel_sender.clone(), "meta");
            spawn_receiver(mcast, data_channel_sender.clone(), "mcast");

            RtpsTransportParticipant {
                message_writer: Box::new(Ipv6MessageWriter { send_sock, scope }),
                default_unicast_locator_list: vec![v6_locator(uni, default_unicast_port)],
                metatraffic_unicast_locator_list: vec![v6_locator(uni, meta_unicast_port)],
                metatraffic_multicast_locator_list: vec![v6_locator(group, sport)],
                default_multicast_locator_list: vec![],
                fragment_size,
            }
        }
    }
}

fn v6_locator(addr: Ipv6Addr, port: u16) -> Locator {
    Locator::new(LOCATOR_KIND_UDP_V6, port as u32, addr.octets())
}

fn local_port(s: &HostUdpSocket) -> u16 {
    s.local_addr().map(|a| a.port()).unwrap_or(0)
}

/// Spawn a blocking receiver thread forwarding each datagram into the core's inbound channel via the
/// seam's async `recv_from`. The whole loop is driven by `block_on` on this dedicated thread:
/// dust_dds's executor is not a tokio reactor, and the host shim's `recv_from` blocks the thread
/// in-poll — which is exactly this thread's job (on the MCU (#328) the identical loop instead
/// suspends on embassy-net's async recv). A spawn failure (OS thread exhaustion) is logged, not
/// panicked — the stack degrades rather than aborting.
fn spawn_receiver(sock: HostUdpSocket, sender: MpscSender<Arc<[u8]>>, tag: &'static str) {
    let spawned = std::thread::Builder::new()
        .name(format!("dds-rx-{tag}"))
        .spawn(move || {
            block_on(async move {
                let mut buf = [0u8; MAX_DATAGRAM];
                loop {
                    match sock.recv_from(&mut buf).await {
                        Ok((n, _)) if n > 0 => {
                            let data: Arc<[u8]> = Arc::from(&buf[..n]);
                            if sender.send(data).await.is_err() {
                                break; // core gone
                            }
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
            });
        });
    if let Err(e) = spawned {
        log::error!("dds: failed to spawn receiver thread `{tag}`: {e}");
    }
}
