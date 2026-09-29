//! What a scripted send-side fault does to a [`super::MockBrokerConn`] (feature `std`).

/// What a [`super::SendFault`] does when it fires.
#[derive(Debug, Clone, Copy)]
pub enum SendFaultKind {
    /// The peer resets the connection right here: bytes up to (not including) `at_offset` are
    /// recorded as sent, and this and every subsequent `send_all` call returns
    /// `SendOutcome::PeerGone`.
    PeerReset,
}
