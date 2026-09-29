//! [`UdpError`] — the portable error surface of the embassy-shaped UDP transport seam.

/// A UDP transport failure, carried across the [`super::udp_endpoint::UdpEndpoint`] seam.
///
/// Deliberately alloc-free and `core`-only (no `std::io::Error`): each fallible operation carries at
/// most a best-effort raw OS error code (`Option<i32>`, the host shim's
/// `std::io::Error::raw_os_error`). This is what lets the same error type serve BOTH the host shim
/// (`embassy-udp-host`) and the future MCU impl (#328), whose errors are embassy-net's
/// `BindError`/`SendError`/`RecvError` enums — not `io::Error` — and carry no errno. The MCU impl
/// maps those enums onto [`UdpError::Truncated`] / a code-less variant; the host shim fills the
/// `Option<i32>` from the kernel. `Display`/`Error` impls live in `impl_udp_error.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UdpError {
    /// Socket creation or `bind` failed (host: socket2 `Socket::new`/`bind`; MCU: embassy-net
    /// `UdpSocket::bind` → `BindError`).
    Bind(Option<i32>),
    /// Joining the SPDP multicast group failed (host: `join_multicast_v6`; MCU:
    /// `Stack::join_multicast_group` → `MulticastError`).
    Join(Option<i32>),
    /// Setting a multicast socket option failed (`IPV6_MULTICAST_IF` / `IPV6_MULTICAST_LOOP`).
    MulticastOption(Option<i32>),
    /// A datagram send failed (host: `send_to`; MCU: embassy-net `UdpSocket::send_to` → `SendError`).
    Send(Option<i32>),
    /// A datagram receive failed (host: `recv_from`).
    Recv(Option<i32>),
    /// The bound local address could not be read.
    LocalAddr(Option<i32>),
    /// The received datagram was larger than the caller's buffer. Mirrors embassy-net's
    /// `RecvError::Truncated` (embassy-net 0.6.0 `udp.rs:37`).
    Truncated,
    /// An address on this seam was not IPv6. This transport is IPv6-only (RTPS
    /// `LOCATOR_KIND_UDP_V6`); an IPv4 peer/local address is a configuration error, never silent.
    NotV6,
}
