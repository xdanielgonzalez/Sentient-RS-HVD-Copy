use super::prelude::*;

impl<const MAX_CLIENTS: usize> BrokerCore<MAX_CLIENTS> {
    /// Parse and act on freshly-received bytes from `slot`'s client. Multiple packets (or partial
    /// packets, reassembled across calls) are handled; each recognized packet is consumed from the
    /// internal reassembly buffer as soon as it is complete. Malformed input (or a slot the caller
    /// never announced via [`Self::client_connected`]) closes the slot and returns
    /// [`BrokerError::Malformed`]/[`BrokerError::InvalidSlot`] — the caller should stop reading that
    /// connection.
    pub fn feed_incoming(&mut self, slot: usize, bytes: &[u8]) -> Result<(), BrokerError> {
        if slot >= MAX_CLIENTS {
            return Err(BrokerError::InvalidSlot);
        }
        {
            let Some(c) = self.clients.get_mut(slot) else {
                return Err(BrokerError::InvalidSlot);
            };
            if !c.occupied || c.closing {
                return Err(BrokerError::InvalidSlot);
            }
            if !bytes.is_empty() && c.rx.extend_from_slice(bytes).is_err() {
                self.mark_closing(slot);
                return Err(BrokerError::Malformed);
            }
        }

        loop {
            // Snapshot into `self.feed_scratch` (persistent scratch storage — see its field doc)
            // rather than a stack-local `heapless::Vec`: this loop is nested inside
            // `feed_incoming`'s caller on the embedded broker-pump thread, and a 512-byte
            // stack-local here stacked with other locals contributed to a past broker-thread stack
            // overflow (see the Phase-2 plan's T3 RAM-budget history).
            match self.clients.get(slot) {
                Some(c) if c.occupied => match heapless::Vec::from_slice(&c.rx) {
                    Ok(v) => self.feed_scratch = v,
                    Err(_) => return Err(BrokerError::Malformed),
                },
                _ => return Ok(()),
            };
            if self.feed_scratch.is_empty() {
                return Ok(());
            }

            let header = match parse_fixed_header(&self.feed_scratch) {
                Ok(h) => h,
                Err(WireError::Truncated) => {
                    // Not enough bytes yet for a complete fixed header — wait for more, UNLESS we
                    // already hold a full RX_CAP worth with still no complete header, which can
                    // only mean the sender is never going to complete it (or is hostile).
                    if self.feed_scratch.len() >= RX_CAP {
                        self.mark_closing(slot);
                        return Err(BrokerError::Malformed);
                    }
                    return Ok(());
                }
                Err(_) => {
                    // A genuine protocol violation (e.g. Remaining Length already exceeds the
                    // spec's 4-byte maximum with the bytes in hand) — not just "need more input".
                    self.mark_closing(slot);
                    return Err(BrokerError::Malformed);
                }
            };
            let total_len = header.header_len.saturating_add(header.remaining_length);
            if total_len > self.feed_scratch.len() {
                if total_len > RX_CAP {
                    // This single packet can never fit in the reassembly buffer.
                    self.mark_closing(slot);
                    return Err(BrokerError::Malformed);
                }
                return Ok(()); // wait for the rest of this packet
            }
            // Own the packet bytes (small stack copy bounded by RX_CAP's remaining-length check
            // above, not the whole reassembly buffer) so the borrow on `self.feed_scratch` ends
            // before `handle_packet` needs `&mut self`.
            let Some(packet_slice) = self.feed_scratch.get(..total_len) else {
                self.mark_closing(slot);
                return Err(BrokerError::Malformed);
            };
            let Ok(packet) = heapless::Vec::<u8, RX_CAP>::from_slice(packet_slice) else {
                self.mark_closing(slot);
                return Err(BrokerError::Malformed);
            };

            let outcome = self.handle_packet(slot, &header, &packet);
            if let Some(c) = self.clients.get_mut(slot) {
                if c.occupied && !c.closing {
                    c.rx_consume(total_len);
                }
            }
            outcome?;
            // handle_packet may have doomed the slot (protocol error, or a clean DISCONNECT) —
            // stop processing further packets in this same feed either way.
            match self.clients.get(slot) {
                Some(c) if c.occupied && !c.closing => {}
                _ => return Ok(()),
            }
        }
    }

    /// Dispatch one complete, already-length-validated packet (`packet` is exactly
    /// `header.header_len + header.remaining_length` bytes: fixed header + variable
    /// header/payload).
    fn handle_packet(
        &mut self,
        slot: usize,
        header: &FixedHeader,
        packet: &[u8],
    ) -> Result<(), BrokerError> {
        let Some(body) = packet.get(header.header_len..) else {
            self.mark_closing(slot);
            return Err(BrokerError::Malformed);
        };
        match header.packet_type {
            PKT_CONNECT => self.handle_connect(slot, body),
            PKT_SUBSCRIBE => self.handle_subscribe(slot, body),
            PKT_PUBLISH => self.handle_publish(slot, header, body),
            PKT_PINGREQ => self.handle_pingreq(slot),
            PKT_DISCONNECT => {
                self.mark_closing(slot);
                Ok(())
            }
            _ => {
                // Any other/unsupported packet type from a minimal-scope client is treated as
                // malformed input for this broker — fail closed rather than silently ignore.
                self.mark_closing(slot);
                Err(BrokerError::Malformed)
            }
        }
    }

    pub(super) fn is_connected(&self, slot: usize) -> bool {
        matches!(self.clients.get(slot), Some(c) if c.occupied && c.connected)
    }

    /// Stage `bytes` as `slot`'s outgoing CONTROL frame; on overflow, drop it and count instead of
    /// blocking/panicking (§ Scope: "On outgoing overflow ... DROP it and increment a counter").
    pub(super) fn stage_or_drop_control(&mut self, slot: usize, bytes: &[u8]) {
        let staged = match self.clients.get_mut(slot) {
            Some(c) if c.occupied => c.stage_control(bytes),
            _ => return,
        };
        if staged.is_err() {
            self.dropped = self.dropped.saturating_add(1);
        }
    }
}
