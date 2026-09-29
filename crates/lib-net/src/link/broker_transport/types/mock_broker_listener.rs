//! A scripted [`super::BrokerListener`] mock: hands out pre-built [`super::MockBrokerConn`]s in
//! order (feature `std` — test-only, never linked into the no_std MCU build).

use super::MockBrokerConn;
use std::collections::VecDeque;

/// A listener that accepts a fixed, scripted sequence of connections: each [`accept`
/// call](super::BrokerListener::accept) pops the next queued [`MockBrokerConn`] (`None` once the
/// queue is empty, or on any tick explicitly scripted as "nothing pending" via
/// [`super::super::MockBrokerListener::no_accept_this_tick`]).
pub struct MockBrokerListener {
    pub(in super::super) queue: VecDeque<Option<MockBrokerConn>>,
}
