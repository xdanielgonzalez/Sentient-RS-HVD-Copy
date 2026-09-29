use lib_net::embassy::udp::HostUdpSocket;

/// The send half of the custom UDP/IPv6 transport: implements dust_dds's `WriteMessage` by sending
/// each already-framed RTPS datagram to its destination `Locator`s over IPv6 (scoping link-local /
/// multicast to the configured interface). The socket is the owned net layer's
/// [`HostUdpSocket`] (the embassy-shaped `UdpEndpoint` seam) — this crate holds no raw sockets. Impl
/// in `impl_message_writer.rs`.
pub struct Ipv6MessageWriter {
    /// The egress UDP/IPv6 endpoint (multicast egress interface set to `scope`). `Clone` (Arc-backed)
    /// so it can move into the boxed send future dust_dds awaits per datagram.
    pub(crate) send_sock: HostUdpSocket,
    /// Interface scope / if-index for link-local + multicast sends.
    pub(crate) scope: u32,
}
