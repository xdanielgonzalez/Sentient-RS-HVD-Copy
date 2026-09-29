//! `impl` blocks for [`super::MockBrokerConn`] (feature `std`).

use super::{
    BrokerConn, MockBrokerConn, MockRecvEvent, RecvOutcome, SendFault, SendFaultKind, SendOutcome,
};

impl MockBrokerConn {
    /// A connection with the given scripted recv events, an unbounded send window, and no
    /// injected fault (every `send_all` succeeds in full immediately).
    pub fn new(recv_script: Vec<MockRecvEvent>) -> Self {
        MockBrokerConn {
            recv_script: recv_script.into(),
            send_window: 0,
            fault: None,
            sent: Vec::new(),
            closed: false,
        }
    }

    /// Constrain every underlying write attempt inside `send_all` to at most `window` bytes,
    /// forcing short-write-resume logic to actually loop.
    pub fn with_send_window(mut self, window: usize) -> Self {
        self.send_window = window;
        self
    }

    /// Inject a send-side fault at the given cumulative offset.
    pub fn with_send_fault(mut self, fault: SendFault) -> Self {
        self.fault = Some(fault);
        self
    }

    /// Append an orderly-close event to the end of the recv script (test helper for scripting a
    /// close on a *later* tick than construction, after some ticks/assertions have already run).
    pub fn recv_script_push_closed(&mut self) {
        self.recv_script.push_back(MockRecvEvent::Closed);
    }
}

impl BrokerConn for MockBrokerConn {
    fn recv(&mut self, buf: &mut [u8]) -> RecvOutcome {
        match self.recv_script.pop_front() {
            Some(MockRecvEvent::Bytes(bytes)) => {
                let n = bytes.len().min(buf.len());
                buf[..n].copy_from_slice(&bytes[..n]);
                RecvOutcome::Bytes(n)
            }
            Some(MockRecvEvent::WouldBlock) => RecvOutcome::WouldBlock,
            Some(MockRecvEvent::Closed) => RecvOutcome::Closed,
            // Script exhausted: steady-state "nothing to read" rather than a hard error, so a
            // test need not script every single tick past the interesting events.
            None => RecvOutcome::WouldBlock,
        }
    }

    /// Resumes across the mock's bounded `send_window` exactly like a real short-write-capable
    /// transport (`net_shim.c`'s `sz_sock_send`, `TcpBrokerConn::send_all`'s `write_all`): loops,
    /// accepting at most `send_window` bytes per underlying "write" attempt, appending each
    /// accepted chunk to `self.sent`, until the whole buffer is sent or an injected
    /// [`SendFaultKind::PeerReset`] fires at its scripted cumulative offset (byte-exact: if the
    /// fault offset falls inside a chunk, only the bytes up to it are accepted before the reset
    /// fires, not the whole chunk).
    fn send_all(&mut self, buf: &[u8]) -> SendOutcome {
        // Already past (or at) the fault offset from a previous call: fail immediately.
        if let Some(fault) = self.fault {
            if self.sent.len() >= fault.at_offset {
                match fault.kind {
                    SendFaultKind::PeerReset => return SendOutcome::PeerGone,
                }
            }
        }

        let mut offset = 0usize;
        while offset < buf.len() {
            let remaining = buf.len() - offset;
            let mut chunk_len = if self.send_window == 0 {
                remaining
            } else {
                remaining.min(self.send_window)
            };
            // Clamp the chunk so it never crosses the fault offset — makes the reset land at an
            // exact byte, not chunk-rounded.
            if let Some(fault) = self.fault {
                if fault.at_offset > self.sent.len() {
                    chunk_len = chunk_len.min(fault.at_offset - self.sent.len());
                }
            }
            self.sent
                .extend_from_slice(&buf[offset..offset + chunk_len]);
            offset += chunk_len;

            if let Some(fault) = self.fault {
                if self.sent.len() >= fault.at_offset {
                    match fault.kind {
                        SendFaultKind::PeerReset => return SendOutcome::PeerGone,
                    }
                }
            }
        }
        SendOutcome::Sent
    }

    fn close(&mut self) {
        self.closed = true;
    }
}
