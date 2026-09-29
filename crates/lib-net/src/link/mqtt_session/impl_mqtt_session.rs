use super::prelude::*;

impl<'a> MqttSession<'a> {
    /// A fresh session for `opts`. The CONNECT is not staged until the first [`Self::poll`]. The
    /// keep-alive interval is taken from `opts.keep_alive_secs` (0 disables it).
    pub fn new(opts: ConnectOptions<'a>) -> Self {
        MqttSession {
            keep_alive_ms: opts.keep_alive_secs as i64 * 1000,
            opts,
            state: State::Init,
            last_tx_ms: 0,
            rx: heapless::Vec::new(),
            tx: heapless::Vec::new(),
            tx_pos: 0,
        }
    }

    /// True once the broker has accepted the connection (CONNACK return code 0).
    pub fn is_connected(&self) -> bool {
        self.state == State::Connected
    }

    /// Advance the session at `now_ms`, consuming any freshly-received plaintext (`incoming`) and
    /// staging outbound plaintext into `out`. Returns how many bytes were written to `out` for the
    /// caller to hand to the transport (0 = nothing to send this poll).
    ///
    /// Drive this from the same bounded poll loop that drives [`crate::link::NetStack::poll`]: feed it
    /// whatever the socket/`TlsStage` produced this iteration and write back whatever it returns.
    pub fn poll(
        &mut self,
        now_ms: i64,
        incoming: &[u8],
        out: &mut [u8],
    ) -> Result<usize, SessionError> {
        // The very first poll stages the CONNECT (nothing is emitted before the caller starts
        // polling, so there is no "connect" method to forget to call).
        if self.state == State::Init {
            let mut scratch = [0u8; TX_CAP];
            let n = encode_connect(&self.opts, &mut scratch)?;
            self.stage(&scratch[..n])?;
            self.state = State::AwaitingConnAck;
            self.last_tx_ms = now_ms;
        }

        if !incoming.is_empty() {
            self.rx
                .extend_from_slice(incoming)
                .map_err(|_| SessionError::BufferOverflow)?;
        }
        self.consume_inbound()?;

        // Keep-alive: once connected, a link idle for a full interval gets a PINGREQ. Bounded — this
        // only fires on a poll whose clock has advanced past the interval.
        if self.state == State::Connected
            && self.keep_alive_ms > 0
            && now_ms - self.last_tx_ms >= self.keep_alive_ms
        {
            let mut ping = [0u8; 2];
            let n = encode_pingreq(&mut ping)?;
            self.stage(&ping[..n])?;
        }

        let written = self.drain(out);
        if written > 0 {
            self.last_tx_ms = now_ms;
        }
        Ok(written)
    }

    /// Stage a QoS 0 PUBLISH of `payload` to `topic`. Fails with [`SessionError::NotConnected`]
    /// until the CONNACK has been accepted (so the wire order — CONNECT, CONNACK, then PUBLISH — is
    /// enforced). The bytes are emitted by the next [`Self::poll`].
    pub fn publish(&mut self, topic: &str, payload: &[u8]) -> Result<(), SessionError> {
        if self.state != State::Connected {
            return Err(SessionError::NotConnected);
        }
        let mut scratch = [0u8; TX_CAP];
        let n = encode_publish(topic, payload, 0, None, &mut scratch)?;
        self.stage(&scratch[..n])
    }

    /// Consume whole control packets sitting at the front of `rx` for the current state.
    fn consume_inbound(&mut self) -> Result<(), SessionError> {
        match self.state {
            State::AwaitingConnAck => {
                // CONNACK is a fixed 4-byte packet (MQTT 3.1.1 section 3.2). Wait for all 4.
                if self.rx.len() >= 4 {
                    let ack = parse_connack(&self.rx[..4]).map_err(|_| {
                        self.state = State::Dead;
                        SessionError::Malformed
                    })?;
                    self.rx_consume(4);
                    if ack.return_code == 0 {
                        self.state = State::Connected;
                    } else {
                        self.state = State::Dead;
                        return Err(SessionError::Refused(ack.return_code));
                    }
                }
            }
            State::Connected => {
                // The only packet the broker sends us on this QoS-0 path is PINGRESP (0xD0 0x00);
                // drop any that have arrived so `rx` doesn't fill. Anything else is left in place.
                while self.rx.len() >= 2 && self.rx[0] == 0xD0 && self.rx[1] == 0x00 {
                    self.rx_consume(2);
                }
            }
            State::Init | State::Dead => {}
        }
        Ok(())
    }

    /// Append `bytes` to the outbound staging buffer, compacting any already-drained prefix first.
    fn stage(&mut self, bytes: &[u8]) -> Result<(), SessionError> {
        self.compact_tx();
        self.tx
            .extend_from_slice(bytes)
            .map_err(|_| SessionError::BufferOverflow)
    }

    /// Copy as much staged output as fits into `out`, advancing the front cursor.
    fn drain(&mut self, out: &mut [u8]) -> usize {
        let available = &self.tx[self.tx_pos..];
        let n = core::cmp::min(out.len(), available.len());
        out[..n].copy_from_slice(&available[..n]);
        self.tx_pos += n;
        if self.tx_pos >= self.tx.len() {
            self.tx.clear();
            self.tx_pos = 0;
        }
        n
    }

    /// Drop the already-drained prefix of `tx`, shifting any remainder to the front.
    fn compact_tx(&mut self) {
        if self.tx_pos == 0 {
            return;
        }
        if self.tx_pos >= self.tx.len() {
            self.tx.clear();
        } else {
            let remaining = self.tx.len() - self.tx_pos;
            self.tx.copy_within(self.tx_pos.., 0);
            self.tx.truncate(remaining);
        }
        self.tx_pos = 0;
    }

    /// Drop the first `n` bytes of `rx`, shifting any remainder to the front.
    fn rx_consume(&mut self, n: usize) {
        let len = self.rx.len();
        if n >= len {
            self.rx.clear();
            return;
        }
        self.rx.copy_within(n.., 0);
        self.rx.truncate(len - n);
    }
}
