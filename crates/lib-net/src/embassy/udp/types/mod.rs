//! Type definitions for the embassy-shaped UDP transport seam. Portable (`core::net`-only) trait +
//! error + bind spec; the host socket type is added under `embassy-udp-host`.

mod udp_bind_spec;
mod udp_endpoint;
mod udp_error;

pub use udp_bind_spec::UdpBindSpec;
pub use udp_endpoint::UdpEndpoint;
pub use udp_error::UdpError;

#[cfg(feature = "embassy-udp-host")]
mod host_udp_socket;
#[cfg(feature = "embassy-udp-host")]
pub use host_udp_socket::HostUdpSocket;
