//! `Display` + `Error` for [`UdpError`]. Portable (`core::fmt` / `core::error`) — imports the type
//! directly rather than through a prelude, since this impl compiles in the no_std seam layer where
//! no std prelude exists.

use core::fmt;

use super::types::UdpError;

impl fmt::Display for UdpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UdpError::Bind(os) => write!(f, "udp bind failed (os error {os:?})"),
            UdpError::Join(os) => write!(f, "udp multicast join failed (os error {os:?})"),
            UdpError::MulticastOption(os) => {
                write!(f, "udp multicast option failed (os error {os:?})")
            }
            UdpError::Send(os) => write!(f, "udp send failed (os error {os:?})"),
            UdpError::Recv(os) => write!(f, "udp recv failed (os error {os:?})"),
            UdpError::LocalAddr(os) => write!(f, "udp local_addr failed (os error {os:?})"),
            UdpError::Truncated => write!(f, "udp datagram larger than receive buffer (truncated)"),
            UdpError::NotV6 => write!(f, "udp address was not IPv6 (this transport is IPv6-only)"),
        }
    }
}

impl core::error::Error for UdpError {}
