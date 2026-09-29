//! `LoopbackBridge` — test-support only: a reusable bridged pair of in-memory
//! [`WireFrame`] queues connecting two [`ChannelDevice`]s, for two-stack loopback tests
//! (TCP/TLS/MQTT e2e).
//!
//! Every unit test living directly in `lib-link` (`device.rs`'s
//! `device_bridges_ingress_and_egress`, `stack.rs`'s `stack_builds_and_polls_without_traffic`)
//! hand-rolls its own `Rc<RefCell<VecDeque<WireFrame>>>` `Sender`/`Receiver` adapter, and it only
//! ever wires a *single* device to itself, never two independent `NetStack`s to each other. This
//! module is the reusable, genuine two-party bridge: `LoopbackBridge::split` hands back two
//! [`ChannelDevice`]s cross-wired so each one's egress is the other's ingress.
//!
//! Formerly the standalone `sentient-net-testkit` crate (retired in #99); merged here so all
//! test-support lives in one crate.

mod impl_bridge_end;
mod impl_loopback_bridge;
mod prelude;
mod types;

pub use prelude::*;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use smoltcp::phy::{Device, RxToken, TxToken};
    use smoltcp::time::Instant;

    #[test]
    fn split_cross_wires_egress_to_the_others_ingress() {
        let (mut device_a, mut device_b) = LoopbackBridge::new(1514).split();

        // Nothing in flight yet.
        assert!(Device::receive(&mut device_a, Instant::ZERO).is_none());
        assert!(Device::receive(&mut device_b, Instant::ZERO).is_none());

        // `a` transmits -> `b` should receive it.
        let tx = Device::transmit(&mut device_a, Instant::ZERO).expect("a can transmit");
        tx.consume(5, |buf| buf.copy_from_slice(b"a->b!"));
        let (rx, _tx) = Device::receive(&mut device_b, Instant::ZERO).expect("b sees a's frame");
        rx.consume(|bytes| assert_eq!(bytes, b"a->b!"));

        // `b` transmits -> `a` should receive it, and `a` doesn't see its own earlier frame again.
        assert!(Device::receive(&mut device_a, Instant::ZERO).is_none());
        let tx = Device::transmit(&mut device_b, Instant::ZERO).expect("b can transmit");
        tx.consume(5, |buf| buf.copy_from_slice(b"b->a!"));
        let (rx, _tx) = Device::receive(&mut device_a, Instant::ZERO).expect("a sees b's frame");
        rx.consume(|bytes| assert_eq!(bytes, b"b->a!"));
    }
}
