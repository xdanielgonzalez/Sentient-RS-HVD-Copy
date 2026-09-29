//! The owned L3+ stack: `smoltcp` (no_std, no-alloc) driven by the channel-backed [`ChannelDevice`].
//!
//! This is the top of the net abstraction: given a [`NetLink`](crate::link::NetLink) driver (whose frames
//! arrive on the internal [`WireFrame`] channel), smoltcp runs IP/TCP and hands the app **sockets**.
//! WireFrames flow up from the driver through the device into smoltcp transparently; the consumer
//! reads/writes a TCP connection and never touches a frame. smoltcp is poll-driven; the platform
//! wraps [`NetStack::poll`] in an async loop that wakes on an ingress frame or the
//! [`NetStack::poll_delay`] timer (embassy on the MCU, tokio on the host) — stackless-coroutine
//! efficiency with no per-task stacks. Fixed-capacity throughout: sockets and buffers are
//! caller-provided slices, no heap.

mod prelude;
mod types;
pub use types::*;

mod impl_net_stack;

pub use smoltcp::iface::SocketHandle;

#[cfg(test)]
use prelude::*;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::channel::{Closed, FlowDir, TryRecvError, TrySendError};
    use core::cell::RefCell;
    use smoltcp::phy::{Device, RxToken, TxToken};
    use std::collections::VecDeque;
    use std::rc::Rc;

    // A shared in-memory channel for tests: both ends share one queue.
    #[derive(Clone)]
    struct TestChan(Rc<RefCell<VecDeque<WireFrame>>>);
    impl TestChan {
        fn new() -> Self {
            TestChan(Rc::new(RefCell::new(VecDeque::new())))
        }
    }
    impl Sender for TestChan {
        type Item = WireFrame;
        fn try_send(&mut self, item: WireFrame) -> Result<(), TrySendError<WireFrame>> {
            self.0.borrow_mut().push_back(item);
            Ok(())
        }
        async fn send(&mut self, item: WireFrame) -> Result<(), Closed> {
            let _ = self.try_send(item);
            Ok(())
        }
    }
    impl Receiver for TestChan {
        type Item = WireFrame;
        fn try_recv(&mut self) -> Result<WireFrame, TryRecvError> {
            self.0.borrow_mut().pop_front().ok_or(TryRecvError::Empty)
        }
        async fn recv(&mut self) -> Option<WireFrame> {
            self.try_recv().ok()
        }
    }

    #[test]
    fn device_bridges_ingress_and_egress() {
        let egress = TestChan::new();
        let ingress = TestChan::new();
        let mut device = ChannelDevice::new(egress.clone(), ingress.clone(), 1514);

        // No ingress → receive yields nothing.
        assert!(Device::receive(&mut device, Instant::ZERO).is_none());

        // Inject an ingress frame → receive yields it; RxToken hands smoltcp the bytes.
        ingress
            .0
            .borrow_mut()
            .push_back(WireFrame::new(FlowDir::Ingress, b"ingress-frame").unwrap());
        let (rx, _tx) = Device::receive(&mut device, Instant::ZERO).expect("a frame is ready");
        rx.consume(|bytes| assert_eq!(bytes, b"ingress-frame"));

        // A TxToken consume enqueues an egress frame the link would transmit.
        let tx = Device::transmit(&mut device, Instant::ZERO).unwrap();
        tx.consume(4, |buf| buf.copy_from_slice(b"eth0"));
        let sent = egress
            .0
            .borrow_mut()
            .pop_front()
            .expect("egress frame enqueued");
        assert_eq!(sent.dir, FlowDir::Egress);
        assert_eq!(sent.data, b"eth0");
    }

    #[test]
    fn stack_builds_and_polls_without_traffic() {
        let mut device = ChannelDevice::new(TestChan::new(), TestChan::new(), 1514);
        let mut storage: [SocketStorage; 2] = Default::default();
        let ip = Ipv4Cidr::new(Ipv4Address::new(192, 168, 1, 50), 24);
        let mut stack = NetStack::new(
            &mut device,
            [2, 0, 0, 0, 0, 1],
            ip,
            Some(Ipv4Address::new(192, 168, 1, 1)),
            &mut storage,
        );

        // A TCP socket can be added and the stack polls cleanly with no frames in flight.
        let mut rx_buf = [0u8; 256];
        let mut tx_buf = [0u8; 256];
        let _h = stack.add_tcp(&mut rx_buf, &mut tx_buf);
        let changed = stack.poll(&mut device, 0);
        assert!(!changed, "no traffic → no socket state change");
        let _ = stack.poll_delay(0);
        let _ = stack.poll(&mut device, 5);
    }
}
