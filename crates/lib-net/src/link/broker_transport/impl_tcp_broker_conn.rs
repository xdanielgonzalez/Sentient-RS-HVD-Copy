//! `impl` blocks for [`super::TcpBrokerConn`] (feature `std`).

use super::{BrokerConn, RecvOutcome, SendOutcome, TcpBrokerConn};
use std::io::{ErrorKind, Read, Write};

impl BrokerConn for TcpBrokerConn {
    /// Nonblocking per-call read: flips the stream nonblocking for the duration of this call
    /// only (mirrors `sz_sock_recv`'s `ZSOCK_MSG_DONTWAIT` — a per-call flag on an otherwise
    /// blocking socket — since `std::net::TcpStream` has no per-call nonblocking-read primitive
    /// on all platforms; toggling around the call gets the same effective semantics). `send_all`
    /// always restores blocking mode first, so this never leaks nonblocking mode across calls.
    fn recv(&mut self, buf: &mut [u8]) -> RecvOutcome {
        if self.stream.set_nonblocking(true).is_err() {
            return RecvOutcome::WouldBlock;
        }
        let outcome = match self.stream.read(buf) {
            Ok(0) => RecvOutcome::Closed,
            Ok(n) => RecvOutcome::Bytes(n),
            Err(e) if e.kind() == ErrorKind::WouldBlock => RecvOutcome::WouldBlock,
            Err(_) => RecvOutcome::WouldBlock,
        };
        // Best-effort restore; `send_all` re-asserts blocking mode regardless, so a failure here
        // (extremely unlikely — the fd is still valid) cannot desync the send-path contract.
        let _ = self.stream.set_nonblocking(false);
        outcome
    }

    /// Blocking full-length write. `std::net::TcpStream::write_all` already resumes across any
    /// legal short writes internally (loops until every byte is confirmed written or an error
    /// occurs), so no manual wrap-the-tail loop is needed here — mirrors `sz_sock_send`'s own
    /// wrap-the-tail loop, just expressed via the stdlib's equivalent primitive.
    fn send_all(&mut self, buf: &[u8]) -> SendOutcome {
        if self.stream.set_nonblocking(false).is_err() {
            return SendOutcome::PeerGone;
        }
        match self.stream.write_all(buf) {
            Ok(()) => SendOutcome::Sent,
            Err(_) => SendOutcome::PeerGone,
        }
    }

    fn close(&mut self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}
