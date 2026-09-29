//! [`HostUdpSocket`]: the `bind` constructor (socket2 setup + multicast) and the embassy-shaped
//! [`UdpEndpoint`] impl over the host kernel stack (feature `embassy-udp-host`).

use super::prelude::*;

impl HostUdpSocket {
    /// Bind and configure one UDP/IPv6 socket per `spec` (the host-runner substrate — see
    /// [`HostUdpSocket`]'s doc for why the Pi uses the kernel stack here). All one-time setup
    /// (`IPV6_V6ONLY`, `SO_REUSEADDR`, the multicast JOIN / `IPV6_MULTICAST_IF` /
    /// `IPV6_MULTICAST_LOOP`) happens here so the [`UdpEndpoint`] surface stays pure send/recv.
    ///
    /// The pre-bind options (`only_v6`, `reuse`) are set on a `socket2::Socket` before `bind`
    /// (std's `UdpSocket::bind` offers no pre-bind hook); the socket is then converted to a
    /// `std::net::UdpSocket` for the safe `&[u8]` / `&mut [u8]` send/recv path (this crate is
    /// `#![forbid(unsafe_code)]`, so socket2's `MaybeUninit` recv buffer is avoided). The one
    /// std-absent option, `IPV6_MULTICAST_IF`, is applied via a safe borrowed `SockRef`.
    pub fn bind(spec: &UdpBindSpec) -> Result<Self, UdpError> {
        let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))
            .map_err(|e| UdpError::Bind(e.raw_os_error()))?;
        socket
            .set_only_v6(spec.only_v6)
            .map_err(|e| UdpError::Bind(e.raw_os_error()))?;
        if spec.reuse {
            socket
                .set_reuse_address(true)
                .map_err(|e| UdpError::Bind(e.raw_os_error()))?;
        }
        socket
            .bind(&SockAddr::from(spec.bind))
            .map_err(|e| UdpError::Bind(e.raw_os_error()))?;

        let sock: UdpSocket = socket.into();

        if let Some(group) = spec.join_multicast {
            sock.join_multicast_v6(&group, spec.scope)
                .map_err(|e| UdpError::Join(e.raw_os_error()))?;
        }
        if let Some(ifindex) = spec.multicast_if {
            SockRef::from(&sock)
                .set_multicast_if_v6(ifindex)
                .map_err(|e| UdpError::MulticastOption(e.raw_os_error()))?;
        }
        if spec.multicast_loop {
            sock.set_multicast_loop_v6(true)
                .map_err(|e| UdpError::MulticastOption(e.raw_os_error()))?;
        }

        Ok(HostUdpSocket {
            sock: Arc::new(sock),
        })
    }
}

impl UdpEndpoint for HostUdpSocket {
    async fn send_to(&self, buf: &[u8], remote: SocketAddrV6) -> Result<(), UdpError> {
        // A kernel UDP send is a single non-suspending syscall. The `async` surface is the
        // embassy-native contract (the MCU impl awaits a real embassy-net send future here); on the
        // host it resolves in one poll with no reactor, so it drives cleanly under dust_dds's own
        // executor when awaited from the boxed `WriteMessage` future.
        self.sock
            .send_to(buf, SocketAddr::V6(remote))
            .map(|_| ())
            .map_err(|e| UdpError::Send(e.raw_os_error()))
    }

    async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddrV6), UdpError> {
        // A blocking kernel recv, resolved in one poll. Callers run this on a DEDICATED receive
        // thread (dust_dds's executor is not a tokio reactor), so the block is that thread's whole
        // job — exactly the point where the MCU impl instead suspends on embassy-net's async recv.
        let (n, from) = self
            .sock
            .recv_from(buf)
            .map_err(|e| UdpError::Recv(e.raw_os_error()))?;
        match from {
            SocketAddr::V6(v6) => Ok((n, v6)),
            SocketAddr::V4(_) => Err(UdpError::NotV6),
        }
    }

    fn local_addr(&self) -> Result<SocketAddrV6, UdpError> {
        match self
            .sock
            .local_addr()
            .map_err(|e| UdpError::LocalAddr(e.raw_os_error()))?
        {
            SocketAddr::V6(v6) => Ok(v6),
            SocketAddr::V4(_) => Err(UdpError::NotV6),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use core::net::Ipv6Addr;

    /// Bind an IPv6 receiver + sender on loopback and prove the embassy-shaped seam round-trips a
    /// datagram byte-exact through `bind` → `send_to` → `recv_from`, reporting the source port and
    /// the bound `local_addr`. Host-only proof of the shim (no DDS stack), per "test on the host
    /// first". `send_to` happens before `recv_from` so the datagram is already queued (the host
    /// shim's recv blocks in-poll); the timeout is a safety net.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn host_udp_v6_loopback_round_trips_a_datagram() {
        let rx = HostUdpSocket::bind(&UdpBindSpec {
            bind: SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0),
            only_v6: true,
            reuse: false,
            join_multicast: None,
            multicast_if: None,
            multicast_loop: false,
            scope: 0,
        })
        .expect("bind rx");
        let rx_addr = rx.local_addr().expect("rx local_addr");
        assert_eq!(*rx_addr.ip(), Ipv6Addr::LOCALHOST);

        let tx = HostUdpSocket::bind(&UdpBindSpec::unicast(0)).expect("bind tx");
        let payload = b"embassy-shaped-udp-seam";
        tx.send_to(payload, rx_addr).await.expect("send_to");

        let mut buf = [0u8; 64];
        let (n, from) =
            tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv_from(&mut buf))
                .await
                .expect("recv timed out")
                .expect("recv_from");

        assert_eq!(&buf[..n], payload);
        assert_eq!(from.port(), tx.local_addr().unwrap().port());
    }
}
