//! [`DrainWriteError`] — the failure signal [`super::super::drain_and_write`] reports.

/// Why [`super::super::drain_and_write`] stopped before draining everything [`crate::link::BrokerCore`] had
/// staged for a slot. There is exactly one failure mode: the underlying
/// [`embedded_io_async::Write`] returned an error on some `out[..n]` packet. Per the frozen
/// `BrokerCore` contract (see `crate::embassy::pump` module docs), a short/failed write means the peer is
/// gone — there is no partial-success case to distinguish, so this carries no payload beyond the
/// fact itself; the caller's response is always the same (`client_closed(slot)` + close the
/// socket), so a richer error here would be signal nobody consumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrainWriteError;
