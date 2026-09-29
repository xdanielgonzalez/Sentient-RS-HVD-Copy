//! [`UdpEndpoint`] — the embassy-shaped async UDP transport seam.

use core::net::SocketAddrV6;

use super::udp_error::UdpError;

/// A bound UDP/IPv6 endpoint, shaped to match `embassy_net::udp::UdpSocket`'s runtime surface so a
/// future MCU impl (#328) is a NATIVE drop-in, not a std-shaped contract embassy has to contort into.
///
/// ## Why this exact shape (embassy-net 0.6.0 `udp.rs`, the version locked in the workspace)
///
/// * [`send_to`](Self::send_to) mirrors `UdpSocket::send_to(&self, buf, remote_endpoint) ->
///   Result<(), SendError>` (`udp.rs:228`) — `async`, `&self`, no returned byte count. The MCU impl
///   forwards straight through; the address maps `SocketAddrV6 → IpEndpoint → UdpMetadata`
///   (`send_to` takes `T: Into<UdpMetadata>`; `IpEndpoint: Into<UdpMetadata>`).
/// * [`recv_from`](Self::recv_from) mirrors `UdpSocket::recv_from<'s>(&'s self, buf: &'s mut [u8]) ->
///   impl Future<Output = Result<(usize, UdpMetadata), RecvError>>` (`udp.rs:137`) — `async`, receive
///   into a caller buffer, return `(len, source)`. The source `UdpMetadata.endpoint` (an
///   `IpEndpoint`) maps back to `SocketAddrV6`; `RecvError::Truncated → UdpError::Truncated`.
/// * [`local_addr`](Self::local_addr) mirrors `UdpSocket::endpoint(&self) -> IpListenEndpoint`
///   (`udp.rs:317`) — synchronous, no I/O.
///
/// **What is NOT on this trait, on purpose.** Construction (buffer provisioning), `bind`, and
/// multicast JOIN are all per-impl setup, not uniform runtime ops, so they live in each impl's
/// constructor (host: [`super::host_udp_socket::HostUdpSocket::bind`] over a [`UdpBindSpec`]; MCU:
/// `UdpSocket::new(stack, rx_meta, rx_buffer, tx_meta, tx_buffer)` + `bind` +
/// `Stack::join_multicast_group`). embassy-net itself splits it the same way: the socket owns
/// send/recv, but a datagram socket is bound via `UdpSocket::bind` and multicast is joined on the
/// `Stack`, not the socket (`Stack::join_multicast_group`, embassy-net 0.6.0 `lib.rs:629`).
///
/// Addresses are `core::net::SocketAddrV6` (portable, no_std, alloc-free) rather than smoltcp's
/// `IpEndpoint`, so the seam does not force smoltcp types on consumers; the `SocketAddrV6 ⇄
/// IpEndpoint` conversion is a one-liner the MCU impl owns. The `scope_id` on a `SocketAddrV6`
/// carries the interface index for link-local / multicast destinations (the host honours it via the
/// kernel; the single-interface MCU stack may ignore it).
///
/// `Send + Sync` so the endpoint can move into a receive task/thread and its futures can satisfy the
/// `Send` bound the dust_dds `WriteMessage` seam requires of the boxed send future.
// `async fn` in a public trait: intentional. Every consumer uses a CONCRETE impl (static dispatch —
// lib-dds holds `HostUdpSocket`, the MCU holds its own socket wrapper), so the futures' `Send`-ness is
// visible at each call site; the trait is never used as `dyn`. This is the shape that mirrors
// embassy-net's own inherent `async fn` UDP surface (see the API citations above); a boxed-future
// variant would be exactly the "contortion" the owner's architecture correction rules out.
#[allow(async_fn_in_trait)]
pub trait UdpEndpoint: Send + Sync {
    /// Send one datagram to `remote` (a fully-formed IPv6 address; its `scope_id` names the egress
    /// interface for link-local / multicast destinations).
    async fn send_to(&self, buf: &[u8], remote: SocketAddrV6) -> Result<(), UdpError>;

    /// Receive one datagram into `buf`, returning `(len, source)`. `len` may be `0` for an empty
    /// datagram; a datagram larger than `buf` yields [`UdpError::Truncated`].
    async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddrV6), UdpError>;

    /// The endpoint's bound local address (for advertising unicast/metatraffic locators).
    fn local_addr(&self) -> Result<SocketAddrV6, UdpError>;
}
