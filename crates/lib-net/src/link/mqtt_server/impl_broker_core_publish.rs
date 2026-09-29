use super::prelude::*;

impl<const MAX_CLIENTS: usize> BrokerCore<MAX_CLIENTS> {
    /// The firmware marks `topic` as published (not from any client): activates it (and marks it
    /// retained, if `retain`) in the active-topic table, then enqueues it onto every currently
    /// subscribed, connected client's pending-publish list — the caller's render closure supplies
    /// the actual bytes at the next [`Self::drain_outgoing`] for each of those clients. **No
    /// payload is taken here** — this call is topic-only (MQTT 3.1.1 section 3.3.1.3 retain
    /// semantics for the retained case; retain-delete via an empty payload is not implemented, see
    /// module docs).
    pub fn publish_topic(&mut self, topic: &str, retain: bool) {
        self.activate(topic, retain);
        self.fan_out(topic, retain);
    }

    pub(super) fn handle_publish(
        &mut self,
        slot: usize,
        header: &FixedHeader,
        body: &[u8],
    ) -> Result<(), BrokerError> {
        if !self.is_connected(slot) {
            self.mark_closing(slot);
            return Err(BrokerError::Malformed);
        }
        let qos = (header.flags >> 1) & 0x03;
        let retain = header.flags & 0x01 != 0;
        let mut pos = 0usize;
        let topic = get_str_field(body, &mut pos).map_err(|_| {
            self.mark_closing(slot);
            BrokerError::Malformed
        })?;
        if qos > 0 {
            // QoS 1/2 (with a packet id field) is out of scope (§ Scope) — refuse rather than
            // silently mis-deliver at the wrong QoS.
            self.mark_closing(slot);
            return Err(BrokerError::Malformed);
        }
        // A client-originated PUBLISH's payload bytes are not retained by this broker either (no
        // payload storage anywhere in this design) — a client publishing into this broker is out of
        // this device's real topology (firmware is the only publisher in practice) but is still
        // parsed correctly and fanned out live via the same topic-only path as `publish_topic`.
        // Fanned-out subscribers render their own bytes from the caller's closure, keyed by topic —
        // a client-originated payload cannot be handed to that closure (it doesn't own a canonical
        // source for it), so fanout here activates the topic (for retain bookkeeping) without a
        // renderable payload; the render closure is expected to supply device-published topics only.
        let _ = body.get(pos..); // parsed for framing correctness; payload bytes are not stored.

        self.activate(topic, retain);
        self.fan_out(topic, retain);
        Ok(())
    }

    /// Enqueue `topic` onto every connected, subscribed client's pending-publish list (topic-only —
    /// see [`PendingPublish`]). Overflow on any one client's list is dropped and counted, never
    /// blocking/panicking; other clients are unaffected.
    fn fan_out(&mut self, topic: &str, retain: bool) {
        for slot in 0..MAX_CLIENTS {
            let matched = match self.clients.get(slot) {
                Some(c) if c.occupied && c.connected => {
                    c.filters.iter().any(|f| topic_matches(f, topic))
                }
                _ => false,
            };
            if !matched {
                continue;
            }
            let Some(c) = self.clients.get_mut(slot) else {
                continue;
            };
            if c.push_pending(topic, retain).is_err() {
                self.dropped = self.dropped.saturating_add(1);
            }
        }
    }

    /// Mark `topic` active (creating a slot if one is free) and, if `retain`, mark it retained.
    /// **No payload bytes involved** — this only tracks the topic string + retained flag.
    fn activate(&mut self, topic: &str, retain: bool) {
        let Ok(owned_topic) = heapless::String::try_from(topic) else {
            return; // topic too long for the active-topic table — silently not tracked.
        };

        if let Some(existing) = self
            .active
            .iter_mut()
            .flatten()
            .find(|s| s.topic == owned_topic)
        {
            if retain {
                existing.retained = true;
            }
            return;
        }
        if let Some(free) = self.active.iter_mut().find(|s| s.is_none()) {
            *free = Some(ActiveTopic {
                topic: owned_topic,
                retained: retain,
            });
        }
        // else: all MAX_RETAINED slots are in use by other topics — silently not tracked (the
        // device's fixed topic set — 3 tms + 1 monitor — never exceeds MAX_RETAINED by design).
    }
}
