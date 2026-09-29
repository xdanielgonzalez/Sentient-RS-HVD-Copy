//! A scripted send-side fault injected into a [`super::MockBrokerConn`] (feature `std`).

use super::send_fault_kind::SendFaultKind;

/// A send-side fault a scripted [`super::MockBrokerConn`] can be told to inject at a given byte
/// offset into the *cumulative* stream sent across all `send_all` calls on this connection.
#[derive(Debug, Clone, Copy)]
pub struct SendFault {
    /// Cumulative byte offset (across all `send_all` calls on this conn) at which the fault
    /// fires.
    pub at_offset: usize,
    /// What the fault does.
    pub kind: SendFaultKind,
}
