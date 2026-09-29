//! A scripted, constrained-transport [`super::BrokerConn`] mock for host test matrices (feature
//! `std`, since it uses `std::vec::Vec`/`std::collections::VecDeque` for unbounded test-side
//! bookkeeping — never linked into the no_std MCU build).

use super::mock_recv_event::MockRecvEvent;
use super::send_fault::SendFault;
use std::collections::VecDeque;

/// A constrained-transport mock connection: a bounded per-write "send window" forces
/// [`super::BrokerConn::send_all`] impls under test to internally resume across short writes
/// (exactly the legal-short-write case `net_shim.c`'s `sz_sock_send` wrap-the-tail loop and
/// `super::TcpBrokerConn::send_all`'s `write_all` both handle), plus scripted recv events and one
/// optional injected send fault (peer reset mid-send).
///
/// Every byte accepted by `send_all` is appended to [`Self::sent`] in order, so tests can assert
/// the exact byte stream the broker pump would have put on the wire — byte-exact, never desynced.
pub struct MockBrokerConn {
    pub(in super::super) recv_script: VecDeque<MockRecvEvent>,
    /// Max bytes accepted per underlying write attempt inside `send_all` — `0` means unbounded
    /// (accept the whole buffer in one attempt). Forces short-write-resume logic to actually
    /// exercise its loop when set below a payload's length.
    pub(in super::super) send_window: usize,
    pub(in super::super) fault: Option<SendFault>,
    /// Cumulative bytes actually accepted by `send_all` across the life of this connection —
    /// tests assert this is byte-exact against what the broker meant to send, and stops exactly
    /// where a `PeerReset` fault fires.
    pub sent: Vec<u8>,
    /// Set true once `close()` is called — tests can assert the pump actually closed the slot
    /// when the contract requires it.
    pub closed: bool,
}
