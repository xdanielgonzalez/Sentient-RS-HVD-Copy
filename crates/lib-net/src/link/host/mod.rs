//! The host L2 link — Linux raw datalink via [`pnet`](pnet_datalink) (AF_PACKET under the hood).
//!
//! The kernel NIC driver still moves L2 bytes, but the kernel TCP/IP stack is bypassed: we
//! receive/transmit **raw Ethernet frames** and run the owned stack (smoltcp → TLS → MQTT) above
//! them. Requires `CAP_NET_RAW` at runtime. pnet owns the socket FFI, so this stays memory-safe —
//! no `unsafe` in our code.
//!
//! pnet's datalink is blocking, so the socket is driven by two dedicated OS threads (an isolated
//! rx pump and tx pump — the "stage as its own thread" model), bridged to the async [`NetLink`]
//! contract by bounded channels.

mod prelude;
mod types;
pub use types::*;

mod impl_host_link;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::link::LinkError;

    #[test]
    fn open_fails_closed_on_unknown_interface() {
        // Unknown interface fails at lookup (no privilege needed); a real interface without
        // CAP_NET_RAW fails at channel creation. Either way `open` returns Err (never panics) —
        // the fail-closed contract. Live frame I/O is on-target T&V (needs CAP_NET_RAW + a NIC).
        assert_eq!(
            HostLink::open("sentient-nonexistent-iface-xyz").err(),
            Some(LinkError::Device)
        );
    }
}
