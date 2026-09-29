use super::prelude::*;

impl<const MAX_CLIENTS: usize> BrokerCore<MAX_CLIENTS> {
    pub(super) fn handle_subscribe(&mut self, slot: usize, body: &[u8]) -> Result<(), BrokerError> {
        if !self.is_connected(slot) {
            self.mark_closing(slot);
            return Err(BrokerError::Malformed);
        }
        let mut pos = 0usize;
        let packet_id = get_u16(body, &mut pos).map_err(|_| {
            self.mark_closing(slot);
            BrokerError::Malformed
        })?;

        // Collect granted-QoS bytes for the SUBACK while walking the filter list; every grant is
        // QoS 0 (0x00) since that's the only delivery this broker implements (MQTT 3.1.1 section
        // 3.8.4 permits granting a lower QoS than requested).
        let mut grants: heapless::Vec<u8, MAX_FILTERS_PER_CLIENT> = heapless::Vec::new();
        let mut new_filters: heapless::Vec<heapless::String<FILTER_CAP>, MAX_FILTERS_PER_CLIENT> =
            heapless::Vec::new();

        while pos < body.len() {
            let filter = get_str_field(body, &mut pos).map_err(|_| {
                self.mark_closing(slot);
                BrokerError::Malformed
            })?;
            let _requested_qos = get_u8(body, &mut pos).map_err(|_| {
                self.mark_closing(slot);
                BrokerError::Malformed
            })?;
            let owned = heapless::String::try_from(filter).map_err(|_| {
                self.mark_closing(slot);
                BrokerError::Malformed
            })?;
            if new_filters.push(owned).is_err() || grants.push(0x00).is_err() {
                self.mark_closing(slot);
                return Err(BrokerError::Malformed);
            }
        }
        if new_filters.is_empty() {
            self.mark_closing(slot);
            return Err(BrokerError::Malformed);
        }

        // Register the filters (bounded by MAX_FILTERS_PER_CLIENT total, not just this SUBSCRIBE).
        if let Some(c) = self.clients.get_mut(slot) {
            for f in &new_filters {
                if c.filters.iter().any(|existing| existing == f) {
                    continue; // already subscribed — MQTT 3.1.1 allows re-subscribe to update QoS.
                }
                // Silently cap at MAX_FILTERS_PER_CLIENT: further filters are still granted (SUBACK
                // must answer every filter in the request) but won't receive fanout. This keeps the
                // client table bounded without panicking or blocking.
                let _ = c.filters.push(f.clone());
            }
        }

        // SUBACK: up to 5 fixed-header bytes (1 type + 4 remaining-length varint) + packet id (2) +
        // one grant byte per filter.
        let mut buf = [0u8; CONTROL_CAP];
        let mut wpos = 0usize;
        let remaining = 2 + grants.len();
        if put_u8(&mut buf, &mut wpos, PKT_SUBACK << 4).is_err()
            || put_remaining_length(&mut buf, &mut wpos, remaining).is_err()
            || put_u16(&mut buf, &mut wpos, packet_id).is_err()
        {
            self.mark_closing(slot);
            return Err(BrokerError::Malformed);
        }
        for &g in &grants {
            if put_u8(&mut buf, &mut wpos, g).is_err() {
                self.mark_closing(slot);
                return Err(BrokerError::Malformed);
            }
        }
        self.stage_or_drop_control(slot, &buf[..wpos]);

        // Retained replay (MQTT 3.1.1 section 3.3.1.3): every RETAINED active topic matching a
        // filter just subscribed to is enqueued onto this client's pending-publish list — the
        // actual bytes are rendered lazily by the caller's closure at the next `drain_outgoing`,
        // not replayed from any stored copy (this broker stores none). Bounded by MAX_RETAINED
        // active-topic slots, so this loop is still bounded.
        for f in &new_filters {
            for active in self.active.iter().flatten() {
                if !active.retained || !topic_matches(f, &active.topic) {
                    continue;
                }
                let Some(c) = self.clients.get_mut(slot) else {
                    continue;
                };
                let already_pending = c.pending.iter().any(|p| p.topic == active.topic);
                if already_pending {
                    continue;
                }
                if c.push_pending(&active.topic, true).is_err() {
                    self.dropped = self.dropped.saturating_add(1);
                }
            }
        }

        Ok(())
    }
}
