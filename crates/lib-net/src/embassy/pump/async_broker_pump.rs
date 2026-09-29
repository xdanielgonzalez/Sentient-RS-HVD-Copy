//! [`async_broker_pump`] — a generic async MQTT broker pump that manages multiple client slots
//! concurrently, coordinates channel-driven device publishes, and emits broker lifecycle events.
//!
//! **Design**: A single `async_broker_pump` future races three futures via `select3`:
//! - **Accept**: obtain a new socket from `accept_source`, place it in an empty slot
//! - **Read**: poll each connected slot for incoming data (round-robin), call `feed_incoming`, drain
//! - **Channel**: receive `MqttUnit`s from the device pipeline, store, publish to all subscribers
//!
//! All three operations coordinate through a shared `BrokerCore<MAX_CLIENTS>`, which is `!Sync`
//! and expects single-threaded/cooperative access. The entire pump runs on one executor thread.

use core::array;
use core::future::Future;
use embedded_io_async::{Read, Write};

use crate::channel::Receiver;
use crate::link::BrokerCore;

use super::{drain_and_write, BrokerEvent, LatestStore};

/// A generic async broker pump managing multiple concurrent client connections, device publishes
/// via channel, and broker event callbacks.
///
/// **Generic parameters:**
/// - `S: Read + Write + Unpin` — the socket type (e.g., `embassy_net::tcp::TcpSocket<'_>`)
/// - `A` — accept-source closure type
/// - `Fut: Future<Output = Option<S>>` — the future type returned by calling the accept closure
/// - `R: Receiver<Item = MqttUnit>` — channel receiver for device publishes
/// - `MAX_CLIENTS` — number of concurrent client slots (default 4)
///
/// **Parameters:**
/// - `accept_source` — `FnMut() -> Fut` where `Fut: Future<Output = Option<S>>`. Called when ready
///   for a new connection; returns `Some(socket)` to place on next available slot, `None` if none
///   ready. Matches `TcpSocket::accept(port)` pattern.
/// - `broker` — shared MQTT broker state (slot-indexed, `!Sync`, single-threaded).
/// - `rx` — channel consumer yielding `MqttUnit` publishes from the device pipeline.
/// - `store` — latest-value store (topic → rendered payload). Pump upserts device publishes here.
/// - `rx_buf` — per-read scratch (handed to `socket.read()`), reused across all slots.
/// - `out` — drain-write scratch (handed to [`drain_and_write`]). Must be ≥
///   [`BrokerCore::min_out_len`] for the largest payload rendered.
/// - `events` — callback `FnMut(BrokerEvent)`, invoked at: `BrokerUp` (entry), `ClientConnect`/
///   `ClientClose` (per-slot lifecycle), `TopicPublish`. Used for defmt/RTT logging.
///
/// **Core loop (using `select!`):**
/// 1. **Accept**: call `accept_source()`, find first empty slot, call `broker.client_connected(slot)`,
///    emit `ClientConnect`.
/// 2. **Read**: try each connected slot in round-robin; on data, call `broker.feed_incoming(slot, ...)`,
///    drain that slot, emit `ClientClose` on EOF/error.
/// 3. **Channel**: call `rx.recv()`, call `store.upsert(topic, payload)`, call
///    `broker.publish_topic(topic, retain=true)`, drain ALL slots, emit `TopicPublish`.
///
/// Drains use the render closure to fetch payloads on-demand from `store`. On EOF/read error:
/// call `broker.client_closed(slot)`, close socket, emit `ClientClose`. On channel close
/// (`rx.recv() → None`): pump exits.
///
/// **Fairness:** Pump tries each slot in round-robin order until one returns data or all would block.
/// Sufficient for typical workloads (1–2 active clients) and avoids manual poll-loop complexity.
///
/// **No allocation, `no_std`:** Sockets stored in fixed-size array. Runs on any executor.
#[allow(clippy::too_many_arguments)]
pub async fn async_broker_pump<
    S,
    A,
    Fut,
    R,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_PAYLOAD_LEN: usize,
>(
    mut accept_source: A,
    broker: &mut BrokerCore<MAX_CLIENTS>,
    rx: &mut R,
    store: &mut LatestStore<MAX_TOPICS, MAX_PAYLOAD_LEN>,
    rx_buf: &mut [u8],
    out: &mut [u8],
    mut events: impl FnMut(BrokerEvent),
) where
    S: Read + Write + Unpin,
    A: FnMut() -> Fut,
    Fut: Future<Output = Option<S>>,
    R: Receiver<Item = crate::channel::message::MqttUnit>,
{
    let mut slots: [Option<S>; MAX_CLIENTS] = array::from_fn(|_| None);
    let mut read_slot_index = 0; // round-robin position for socket reads

    events(BrokerEvent::BrokerUp);

    loop {
        // Helper future: try to read from one connected slot (round-robin).
        let read_fut = async {
            let start = read_slot_index;
            loop {
                let slot = read_slot_index;
                read_slot_index = (read_slot_index + 1) % MAX_CLIENTS;

                if let Some(socket) = slots[slot].as_mut() {
                    return Some((slot, socket.read(rx_buf).await));
                }

                if read_slot_index == start {
                    // Checked all slots, none connected
                    break;
                }
            }
            // No slot holds a socket right now: do NOT resolve. If this returned `None` here,
            // `read_fut` would be `Ready` on every poll with zero (or all-momentarily-empty)
            // slots, so `select3` below would win on this branch on EVERY poll without ever
            // awaiting anything real — the outer `loop` would spin the executor at 100% CPU and
            // never yield, starving every other task (e.g. the embassy-net runner tasks never
            // get polled, and the device becomes unreachable). `pending()` never completes, so
            // this future genuinely suspends and `select3` instead blocks on `accept_source()`/
            // `rx.recv()` as intended. Once a client connects, the outer loop repeats and builds
            // a FRESH `read_fut` that sees the new slot — so this is not a permanent block.
            core::future::pending::<Option<_>>().await
        };

        // Race accept, channel, and read
        use embassy_futures::select::select3;

        match select3(accept_source(), rx.recv(), read_fut).await {
            embassy_futures::select::Either3::First(Some(socket)) => {
                // Accepted a new connection
                if let Some(slot) = slots.iter().position(|s| s.is_none()) {
                    slots[slot] = Some(socket);
                    broker.client_connected(slot);
                    events(BrokerEvent::ClientConnect);
                }
            }

            embassy_futures::select::Either3::First(None) => {
                // accept_source() returned None; loop back
            }

            embassy_futures::select::Either3::Second(Some(unit)) => {
                // Channel delivered a unit
                let _ = store.upsert(unit.topic.as_str(), &unit.payload);
                broker.publish_topic(unit.topic.as_str(), true);
                events(BrokerEvent::TopicPublish);

                // Drain all connected slots (create render closure for this drain phase only)
                {
                    let mut render = |topic: &str, buf: &mut [u8]| -> Option<usize> {
                        store.get(topic).map(|payload| {
                            let n = payload.len().min(buf.len());
                            if n > 0 {
                                buf[..n].copy_from_slice(&payload[..n]);
                            }
                            n
                        })
                    };
                    for (slot, socket) in slots.iter_mut().enumerate().take(MAX_CLIENTS) {
                        if let Some(socket) = socket {
                            let _ = drain_and_write(broker, slot, socket, out, &mut render).await;
                        }
                    }
                }
            }

            embassy_futures::select::Either3::Second(None) => {
                // Channel closed; pump exits
                break;
            }

            embassy_futures::select::Either3::Third(Some((slot, Ok(0)))) => {
                // Orderly close (EOF)
                broker.client_closed(slot);
                slots[slot] = None;
                events(BrokerEvent::ClientClose);
            }

            embassy_futures::select::Either3::Third(Some((slot, Ok(n)))) => {
                // Read succeeded: feed, then drain that slot (create render closure for this drain)
                let _ = broker.feed_incoming(slot, &rx_buf[..n]);
                if let Some(socket) = slots[slot].as_mut() {
                    let mut render = |topic: &str, buf: &mut [u8]| -> Option<usize> {
                        store.get(topic).map(|payload| {
                            let n = payload.len().min(buf.len());
                            if n > 0 {
                                buf[..n].copy_from_slice(&payload[..n]);
                            }
                            n
                        })
                    };
                    if drain_and_write(broker, slot, socket, out, &mut render)
                        .await
                        .is_err()
                    {
                        broker.client_closed(slot);
                        slots[slot] = None;
                        events(BrokerEvent::ClientClose);
                    }
                }
            }

            embassy_futures::select::Either3::Third(Some((slot, Err(_)))) => {
                // Read error; close slot
                broker.client_closed(slot);
                slots[slot] = None;
                events(BrokerEvent::ClientClose);
            }

            embassy_futures::select::Either3::Third(None) => {
                // Unreachable: `read_fut` never resolves to `None` (it `pending()`s instead when
                // no slot holds a socket — see its definition above), so `select3` can never pick
                // this branch with a `None` payload. Kept (not `unreachable!()`) as a harmless,
                // cheap-to-keep fallback rather than an assertion that would turn a future,
                // unrelated change to `read_fut`'s shape into a panic instead of a compile error.
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use embedded_io_async::ErrorType;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    use crate::channel::message::MqttUnit;
    use crate::channel::TryRecvError;

    #[derive(Debug)]
    struct MockIoError;
    // embedded-io 0.7 requires `core::error::Error` + `Display` on the mock error (see
    // `drain_write.rs`'s own mock error for the same note).
    impl core::fmt::Display for MockIoError {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("mock io error")
        }
    }
    impl core::error::Error for MockIoError {}
    impl embedded_io_async::Error for MockIoError {
        fn kind(&self) -> embedded_io_async::ErrorKind {
            embedded_io_async::ErrorKind::Other
        }
    }

    /// A socket that never completes a read/write — used only to satisfy `async_broker_pump`'s
    /// `S: Read + Write + Unpin` generic bound in tests that never actually connect a client (the
    /// zero-clients busy-spin bug this module fixes needs no real socket at all).
    struct NeverSocket;
    impl ErrorType for NeverSocket {
        type Error = MockIoError;
    }
    impl embedded_io_async::Read for NeverSocket {
        async fn read(&mut self, _buf: &mut [u8]) -> Result<usize, Self::Error> {
            core::future::pending().await
        }
    }
    impl embedded_io_async::Write for NeverSocket {
        async fn write(&mut self, _buf: &[u8]) -> Result<usize, Self::Error> {
            core::future::pending().await
        }
        async fn flush(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    /// A mock [`Receiver`] backed by a shared (`Rc<RefCell<..>>`) queue: `recv()` returns the next
    /// queued item immediately if one is present, `None` once `close()`'d and drained, or suspends
    /// (`pending()`) otherwise — mirroring a real bounded channel's shape closely enough to drive
    /// `async_broker_pump`. Cheaply `Clone`, so a test keeps a `handle` to push items / close the
    /// channel from OUTSIDE the pump's `&mut` borrow while the pump future is alive and pinned.
    #[derive(Clone)]
    struct QueueReceiver<T> {
        queue: Rc<RefCell<VecDeque<T>>>,
        closed: Rc<RefCell<bool>>,
    }

    impl<T> QueueReceiver<T> {
        fn new() -> Self {
            QueueReceiver {
                queue: Rc::new(RefCell::new(VecDeque::new())),
                closed: Rc::new(RefCell::new(false)),
            }
        }

        fn push(&self, item: T) {
            self.queue.borrow_mut().push_back(item);
        }

        fn close(&self) {
            *self.closed.borrow_mut() = true;
        }
    }

    impl<T> Receiver for QueueReceiver<T> {
        type Item = T;

        fn try_recv(&mut self) -> Result<T, TryRecvError> {
            self.queue
                .borrow_mut()
                .pop_front()
                .ok_or(TryRecvError::Empty)
        }

        async fn recv(&mut self) -> Option<T> {
            // `poll_fn` re-runs this closure on EVERY poll of the returned future — including
            // polls of an already-pending, previously-parked instance (exactly a real channel's
            // shape: a later push must be observed by the NEXT poll of the same still-suspended
            // `recv()`, not just future ones). A naive `if let ... { } else { ...pending().await }`
            // body would instead commit to `pending()` on the FIRST poll and never look at the
            // queue again for that future instance — invisible to a later `push()` — AND would
            // hold the `.borrow_mut()`/`.borrow()` guard alive across the suspension (a classic
            // temporary-lifetime-extension trap), double-panicking the `RefCell` the next time a
            // test pushes from outside while this future is parked mid-`poll`. `poll_fn`'s
            // closure body runs and returns synchronously on each call, so no guard ever survives
            // a suspension point.
            core::future::poll_fn(|_cx| {
                if let Some(item) = self.queue.borrow_mut().pop_front() {
                    return Poll::Ready(Some(item));
                }
                if *self.closed.borrow() {
                    return Poll::Ready(None);
                }
                Poll::Pending
            })
            .await
        }
    }

    /// A `Waker` that does nothing on `wake()` — sufficient here because the test drives
    /// `Future::poll` directly (never relying on a real executor to re-poll on notification).
    struct NoopWaker;
    impl Wake for NoopWaker {
        fn wake(self: Arc<Self>) {}
    }
    fn noop_waker() -> Waker {
        Waker::from(Arc::new(NoopWaker))
    }

    fn unit(topic: &str, payload: &[u8]) -> MqttUnit {
        let mut m = MqttUnit {
            topic: heapless::String::new(),
            payload: heapless::Vec::new(),
            qos: 0,
        };
        m.topic.push_str(topic).expect("topic fits");
        m.payload.extend_from_slice(payload).expect("payload fits");
        m
    }

    /// Regression test for the busy-spin bug: with zero connected client slots, `read_fut` used to
    /// resolve `Ready(None)` on EVERY poll (no `.await` ever taken), so `select3` picked that
    /// branch unconditionally and the outer `loop` never yielded — the top-level `Future::poll`
    /// call for this pump would never return `Poll::Pending`, spinning the calling
    /// thread/executor forever (starving every other task, e.g. the embassy-net runner tasks,
    /// which is what made the device unreachable on real hardware).
    ///
    /// This drives the pump with a hand-rolled no-op `Waker` and calls `Future::poll` directly
    /// (never `.await`), a small fixed number of times. That is deliberate: if the busy-spin
    /// regressed, a SINGLE one of these `poll()` calls would never return at all (the spin is a
    /// tight loop *inside* one `poll()` invocation, not merely repeated polling from outside — see
    /// this fn's own doc above) and this test would hang the process rather than fail cleanly, so
    /// every assertion below only makes sense — and only completes — if the fix holds.
    #[test]
    fn zero_clients_does_not_livelock_and_makes_bounded_progress() {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        let events = Rc::new(RefCell::new(Vec::new()));
        let events_cb = {
            let events = events.clone();
            move |e: BrokerEvent| events.borrow_mut().push(e)
        };

        let mut rx = QueueReceiver::<MqttUnit>::new();
        let rx_handle = rx.clone(); // kept to push/close from outside the pump's exclusive borrow
        rx_handle.push(unit("t/a", b"one"));
        rx_handle.push(unit("t/b", b"two"));

        let mut broker: BrokerCore<4> = BrokerCore::new();
        let mut store: LatestStore = LatestStore::new();
        let mut rx_buf = [0u8; 64];
        let mut out = [0u8; 64];

        // Never yields a client: the pump must survive its entire run with zero connected slots,
        // which is exactly the scenario that used to busy-spin.
        let accept_source = || async { core::future::pending::<Option<NeverSocket>>().await };

        let fut = async_broker_pump::<NeverSocket, _, _, _, 4, 32, 8192>(
            accept_source,
            &mut broker,
            &mut rx,
            &mut store,
            &mut rx_buf,
            &mut out,
            events_cb,
        );
        let mut fut = Box::pin(fut);

        let publish_count = |events: &Rc<RefCell<Vec<BrokerEvent>>>| -> usize {
            events
                .borrow()
                .iter()
                .filter(|e| **e == BrokerEvent::TopicPublish)
                .count()
        };

        // Poll 1: both queued units are immediately available (no real waiting needed to fetch
        // them from the mock channel), so this single `poll()` call drains both and then — with
        // zero client slots and nothing left to do — must return `Pending`, not spin.
        assert_eq!(
            fut.as_mut().poll(&mut cx),
            Poll::Pending,
            "must suspend, not busy-spin, with zero connected clients"
        );
        assert_eq!(
            publish_count(&events),
            2,
            "both queued units processed by the first poll"
        );

        // Poll 2: nothing changed (queue empty, no client, channel open) — must stay `Pending`
        // with no new events, proving repeated polls don't spin or do phantom work either.
        assert_eq!(fut.as_mut().poll(&mut cx), Poll::Pending);
        assert_eq!(publish_count(&events), 2);

        // Poll 3: push one more unit BETWEEN polls (simulating the device pipeline waking the
        // pump later) — proves the pump resumes correctly and is not permanently blocked by the
        // fix (matches this module's own doc: "this is correct, not a permanent block").
        rx_handle.push(unit("t/c", b"three"));
        assert_eq!(fut.as_mut().poll(&mut cx), Poll::Pending);
        assert_eq!(publish_count(&events), 3);

        // Poll 4: close the channel — the pump must exit cleanly (`Ready`) rather than hang,
        // completing the whole run within a small, bounded number of polls.
        rx_handle.close();
        assert_eq!(
            fut.as_mut().poll(&mut cx),
            Poll::Ready(()),
            "pump exits once the channel closes, rather than hanging"
        );

        assert_eq!(events.borrow().first(), Some(&BrokerEvent::BrokerUp));
    }
}
