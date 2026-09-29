//! The two pump tasks: [`pump_link_to_channel`] (ingress) and [`pump_channel_to_link`] (egress).
//! Free functions — no type/impl split (see the repo's code-organization rule): there is no struct
//! here, only the two async loops, mirroring [`crate::link::pump_ingress`]/[`crate::link::pump_egress`]
//! (also bare functions in that crate) with the wire-byte channel swapped for the driver-channel
//! `RxRunner`/`TxRunner` halves.

use crate::channel::WireFrame;
use crate::link::{LinkError, NetLink};
use embassy_net_driver_channel::{RxRunner, TxRunner};

/// Ingress pump: `link.recv()` -> the driver-channel RX queue, forever. Stops when the link goes
/// [`LinkError::Down`]. Transient link errors (`Overrun`/`Device`) are skipped, matching
/// [`crate::link::pump_ingress`]'s behavior.
pub async fn pump_link_to_channel<L, const MTU: usize>(mut link: L, mut rx: RxRunner<'_, MTU>)
where
    L: NetLink,
{
    loop {
        let mut frame = WireFrame::empty(crate::channel::FlowDir::Ingress);
        match link.recv(&mut frame).await {
            Ok(()) => {
                let len = frame.data.len().min(MTU);
                let buf = rx.rx_buf().await;
                buf[..len].copy_from_slice(&frame.data[..len]);
                rx.rx_done(len);
            }
            Err(LinkError::Down) => break,
            Err(_) => {} // transient — keep receiving
        }
    }
}

/// Egress pump: the driver-channel TX queue -> `link.send()`, forever. Stops when the link goes
/// [`LinkError::Down`]. A failed transmit is dropped (the stack above retransmits), matching
/// [`crate::link::pump_egress`]'s behavior.
pub async fn pump_channel_to_link<L, const MTU: usize>(mut link: L, mut tx: TxRunner<'_, MTU>)
where
    L: NetLink,
{
    loop {
        let buf = tx.tx_buf().await;
        // WireFrame::new fails closed (None) if the frame exceeds WIRE_MTU; MTU is caller-chosen
        // to fit within it (see StackConfig), so this only trips on a caller misconfiguration —
        // dropping (not panicking) keeps the pump alive rather than trusting the byte count blind.
        let sent = match WireFrame::new(crate::channel::FlowDir::Egress, buf) {
            Some(frame) => link.send(&frame).await,
            None => Ok(()),
        };
        tx.tx_done();
        if let Err(LinkError::Down) = sent {
            break;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::link::LoopbackLink;
    use core::task::Context;
    use embassy_net_driver::HardwareAddress;
    use std::sync::{Arc, Mutex};

    const TEST_MTU: usize = 1514;

    fn noop_cx() -> Context<'static> {
        // A no-op waker (stable `Waker::noop` since Rust 1.85, well under this workspace's 1.91
        // MSRV — no `unsafe` needed, matching this crate's `forbid(unsafe_code)`): these tests
        // drive the driver-channel's synchronous receive/transmit (already-ready slots), never
        // actually parking on the waker.
        Context::from_waker(std::task::Waker::noop())
    }

    #[tokio::test]
    async fn ingress_pump_moves_a_frame_from_link_to_channel() {
        let mut link: LoopbackLink<4> = LoopbackLink::new([2, 0, 0, 0, 0, 9]);
        link.preload(b"hello-embassy").unwrap();

        let mut state: embassy_net_driver_channel::State<TEST_MTU, 4, 4> =
            embassy_net_driver_channel::State::new();
        let (runner, mut device) = embassy_net_driver_channel::new(
            &mut state,
            HardwareAddress::Ethernet([2, 0, 0, 0, 0, 9]),
        );
        let (_state_runner, rx_runner, _tx_runner) = runner.split();

        // The pump ends when LoopbackLink's queue drains (reports Down) — same contract as
        // crate::link::pump_ingress.
        pump_link_to_channel::<_, TEST_MTU>(link, rx_runner).await;

        // The frame landed in the channel's RX side, ready for embassy-net's Device::receive.
        let mut cx = noop_cx();
        match embassy_net_driver::Driver::receive(&mut device, &mut cx) {
            Some((rx_token, _tx_token)) => {
                embassy_net_driver::RxToken::consume(rx_token, |bytes| {
                    assert_eq!(bytes, b"hello-embassy");
                });
            }
            None => panic!("expected a queued rx frame"),
        }
    }

    /// A `NetLink` test double that records every `send`ed frame's bytes into a shared `Vec` and
    /// never reports `Down` — the egress pump under test is expected to block forever on its
    /// SECOND `tx.tx_buf().await` (nothing else is ever queued), so the test races it against a
    /// timeout and asserts on what was recorded before the deadline, rather than expecting the
    /// pump to return on its own.
    struct RecordingLink {
        sent: Arc<Mutex<Vec<std::vec::Vec<u8>>>>,
        mac: [u8; 6],
    }

    impl NetLink for RecordingLink {
        async fn recv(&mut self, _out: &mut WireFrame) -> Result<(), LinkError> {
            // The egress-only test never calls recv; the trait still requires an impl.
            Err(LinkError::Down)
        }
        async fn send(&mut self, frame: &WireFrame) -> Result<(), LinkError> {
            self.sent.lock().unwrap().push(frame.data.to_vec());
            Ok(())
        }
        fn mac(&self) -> [u8; 6] {
            self.mac
        }
    }

    #[tokio::test]
    async fn egress_pump_moves_a_frame_from_channel_to_link() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let link = RecordingLink {
            sent: sent.clone(),
            mac: [2, 0, 0, 0, 0, 10],
        };

        let mut state: embassy_net_driver_channel::State<TEST_MTU, 4, 4> =
            embassy_net_driver_channel::State::new();
        let (runner, mut device) = embassy_net_driver_channel::new(
            &mut state,
            HardwareAddress::Ethernet([2, 0, 0, 0, 0, 10]),
        );
        let (_state_runner, _rx_runner, tx_runner) = runner.split();

        // Queue one frame for transmit via the Device::TxToken interface (as embassy-net would).
        let mut cx = noop_cx();
        let tx_token =
            embassy_net_driver::Driver::transmit(&mut device, &mut cx).expect("tx slot available");
        embassy_net_driver::TxToken::consume(tx_token, 5, |buf| {
            buf.copy_from_slice(b"world");
        });

        // The pump never returns on its own here (see RecordingLink's doc); bound it so the test
        // itself terminates once the one queued frame has demonstrably crossed to the link.
        let _ = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            pump_channel_to_link::<_, TEST_MTU>(link, tx_runner),
        )
        .await;

        assert_eq!(sent.lock().unwrap().as_slice(), &[b"world".to_vec()]);
    }
}
