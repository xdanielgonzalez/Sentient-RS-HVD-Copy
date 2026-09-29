use crate::channel::WireFrame;

/// An in-memory L2 link for tests and bring-up: [`send`](crate::link::NetLink::send) enqueues a frame,
/// [`recv`](crate::link::NetLink::recv) dequeues the next one (so it loops back), and an empty queue reports
/// [`LinkError::Down`](crate::link::LinkError::Down) (ends a pump deterministically). `N` bounds the in-flight queue.
pub struct LoopbackLink<const N: usize> {
    pub(crate) queue: heapless::Deque<WireFrame, N>,
    pub(crate) mac: [u8; 6],
}
