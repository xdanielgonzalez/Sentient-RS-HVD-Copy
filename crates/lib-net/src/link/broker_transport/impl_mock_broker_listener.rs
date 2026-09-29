//! `impl` blocks for [`super::MockBrokerListener`] (feature `std`).

use super::{BrokerListener, MockBrokerConn, MockBrokerListener};
use std::collections::VecDeque;

impl MockBrokerListener {
    /// A listener with no connections queued yet.
    pub fn new() -> Self {
        MockBrokerListener {
            queue: VecDeque::new(),
        }
    }

    /// Queue a connection to be handed out on a future `accept` call.
    pub fn queue_connection(&mut self, conn: MockBrokerConn) {
        self.queue.push_back(Some(conn));
    }

    /// Queue an explicit "nothing pending this tick" — lets a test interleave accept-misses
    /// between real connections.
    pub fn no_accept_this_tick(&mut self) {
        self.queue.push_back(None);
    }
}

impl Default for MockBrokerListener {
    fn default() -> Self {
        Self::new()
    }
}

impl BrokerListener for MockBrokerListener {
    type Conn = MockBrokerConn;

    fn accept(&mut self) -> Option<Self::Conn> {
        // Queue exhausted, or the next scripted tick is an explicit accept-miss (`None` pushed
        // by `no_accept_this_tick`) — both collapse to "nothing pending" via `flatten`.
        self.queue.pop_front().flatten()
    }
}
