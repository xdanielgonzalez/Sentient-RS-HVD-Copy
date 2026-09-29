//! [`StackConfig`] — the static network parameters for the M1 bring-up stack.

use embassy_net::Ipv4Cidr;

/// The L2 (Ethernet) MTU the driver-channel adapter and `embassy_net::Stack` both use — must fit
/// within [`crate::channel::message::WIRE_MTU`] (the `WireFrame` boundary's own bound), which it does
/// (1514 == 1514; the MTU is not independently choosable per-caller here, it's the wire boundary's
/// own ceiling).
pub const MTU: usize = crate::channel::message::WIRE_MTU;
/// Inbound driver-channel packet queue depth (frames buffered between the link's ingress pump and
/// embassy-net's poll).
pub const N_RX: usize = 4;
/// Outbound driver-channel packet queue depth.
pub const N_TX: usize = 4;

/// Static IPv4 network config for the M1 bring-up (`192.0.2.1/24` — TEST-NET-1, RFC 5737 — chosen
/// deliberately so this spike never collides with a real LAN address).
pub struct StackConfig {
    /// The stack's own address + prefix length.
    pub cidr: Ipv4Cidr,
    /// The MAC address embassy-net's `Device` reports (also the [`crate::link::NetLink`] driver's).
    pub mac: [u8; 6],
}
