//! One scripted inbound event for [`super::MockBrokerConn`] (feature `std`).

/// One scripted inbound event, consumed in order by `super::MockBrokerConn::recv`.
#[derive(Debug, Clone)]
pub enum MockRecvEvent {
    /// Deliver these bytes as one `recv` call's result (a `RecvOutcome::Bytes(n)`, `n =
    /// bytes.len()`).
    Bytes(Vec<u8>),
    /// Nothing available this tick (`RecvOutcome::WouldBlock`).
    WouldBlock,
    /// Orderly peer close (`RecvOutcome::Closed`).
    Closed,
}
