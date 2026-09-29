//! Private prelude for the host-shim `impl_*.rs` files (feature `embassy-udp-host`) ONLY. The
//! portable impls (`impl_udp_error`, `impl_udp_bind_spec`) and every `types/` file import directly —
//! only the host shim, which pulls std + socket2, uses this glob.

pub use super::types::*;

pub use core::net::{SocketAddr, SocketAddrV6};
pub use std::net::UdpSocket;
pub use std::sync::Arc;

pub use socket2::{Domain, Protocol, SockAddr, SockRef, Socket, Type};
