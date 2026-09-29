use super::prelude::*;

impl<const MAX_CLIENTS: usize> BrokerCore<MAX_CLIENTS> {
    /// Render and copy at most one packet's worth of bytes staged for `slot` into `out`, returning
    /// how many were written (0 if the slot is unknown/unoccupied, or has nothing staged/pending).
    ///
    /// Behavior, in priority order:
    /// 1. If `slot` has a staged CONTROL packet (CONNACK/SUBACK/PINGRESP), frame it into `out` and
    ///    return its length. Control packets need no render — they carry no application payload.
    /// 2. Otherwise, if `slot` has a pending-publish topic, call `render(topic, payload_region)`
    ///    where `payload_region` is the tail of `out` reserved for the payload. `render` returns
    ///    `Some(len)` — the number of bytes it wrote into `payload_region[..len]` — or `None` if
    ///    there's no data for that topic right now. On `Some`, a QoS-0 PUBLISH (fixed header, topic,
    ///    rendered payload) is framed into `out` and its total length returned. On `None`, that
    ///    pending entry is dropped (never retried) and this call returns 0.
    ///
    /// At most one packet is produced per call — callers loop until it returns 0. `out` must be at
    /// least [`Self::min_out_len`] bytes for the largest packet this broker can ever produce; a
    /// smaller `out` simply yields 0 (nothing dropped — the entry stays pending) rather than
    /// panicking or writing a truncated packet, EXCEPT the render callback itself is always offered
    /// the *entire* remaining capacity of `out` past the reserved header/topic region, so a caller
    /// sizing `out` to its own maximum payload size (rather than exactly `min_out_len`) gets that
    /// full capacity passed to `render`.
    ///
    /// ## Render-into-`out` framing
    ///
    /// To avoid a second payload buffer, the payload is rendered **directly into `out`** at its
    /// final offset: the fixed header (up to 5 bytes: 1 type/flags + up to 4 Remaining Length varint
    /// bytes) and the topic (2-byte length prefix + topic bytes) are provisionally written assuming
    /// a worst-case 4-byte Remaining Length varint, reserving `5 + 2 + topic.len()` bytes; `render`
    /// then writes the payload starting at that offset. Once the payload length is known, the
    /// Remaining Length varint is re-encoded (it may take fewer than 4 bytes) and the whole frame
    /// (header + topic + payload) is shifted left within `out` to close the gap, so `out[..n]` is
    /// exactly the wire-correct packet with no padding.
    pub fn drain_outgoing<R>(&mut self, slot: usize, out: &mut [u8], render: &mut R) -> usize
    where
        R: FnMut(&str, &mut [u8]) -> Option<usize>,
    {
        let Some(c) = self.clients.get_mut(slot) else {
            return 0;
        };
        if !c.occupied {
            return 0;
        }

        if let Some(frame) = c.control.take() {
            let n = frame.bytes.len();
            let Some(dst) = out.get_mut(..n) else {
                // Doesn't fit: put it back rather than losing it (never drop a control frame
                // silently — it's the only ack the client gets for CONNECT/SUBSCRIBE/PINGREQ).
                c.control = Some(frame);
                return 0;
            };
            dst.copy_from_slice(&frame.bytes);
            return n;
        }

        loop {
            if c.pending.is_empty() {
                return 0;
            }
            let entry = c.pending.remove(0);
            match Self::render_publish(out, &entry.topic, entry.retain, render) {
                Some(n) => return n,
                None => continue, // render said "nothing right now" — drop, try the next pending entry.
            }
        }
    }

    /// Minimum `out` length [`Self::drain_outgoing`] needs to ever produce a full packet, given the
    /// caller's own maximum rendered-payload size `max_payload_len`. = 5 (worst-case fixed header:
    /// 1 type/flags + 4 Remaining Length varint bytes) + 2 (topic length prefix) + `TOPIC_CAP`
    /// (worst-case topic) + `max_payload_len`.
    pub const fn min_out_len(max_payload_len: usize) -> usize {
        5 + 2 + TOPIC_CAP + max_payload_len
    }

    /// Render one QoS-0 PUBLISH for `topic` directly into `out`, returning the total framed length,
    /// or `None` if `render` reported no data (or the topic/header didn't fit `out` at all).
    ///
    /// Layout while rendering: reserves the worst-case header (5 bytes) + topic length-prefix (2) +
    /// `topic` at the front of `out`, offers `render` the remaining tail of `out` as the payload
    /// region, then backfills the *actual* (possibly shorter) Remaining Length varint and shifts the
    /// topic+payload left to close any gap left by a shorter-than-worst-case varint.
    fn render_publish<R>(out: &mut [u8], topic: &str, retain: bool, render: &mut R) -> Option<usize>
    where
        R: FnMut(&str, &mut [u8]) -> Option<usize>,
    {
        const MAX_HEADER: usize = 5; // 1 type/flags byte + up to 4 Remaining Length varint bytes.
        let topic_bytes = topic.as_bytes();
        let topic_field_len = 2 + topic_bytes.len(); // 2-byte length prefix + topic bytes.
        let payload_off = MAX_HEADER.checked_add(topic_field_len)?;
        if payload_off > out.len() {
            return None; // doesn't fit even before rendering — never call render on a doomed frame.
        }

        // Write the topic length-prefix + bytes at the worst-case fixed offset now; the fixed
        // header itself is backfilled below once the final Remaining Length is known.
        let mut wpos = MAX_HEADER;
        put_str_field(out, &mut wpos, topic_bytes).ok()?;
        debug_assert_eq!(wpos, payload_off);

        let payload_region = out.get_mut(payload_off..)?;
        let payload_len = render(topic, payload_region)?;
        if payload_off.checked_add(payload_len)? > out.len() {
            return None; // render reported more bytes than the region it was given — reject.
        }

        let remaining = topic_field_len + payload_len;
        let mut header = [0u8; MAX_HEADER];
        let mut hpos = 0usize;
        let first = (PKT_PUBLISH << 4) | if retain { 0x01 } else { 0x00 };
        put_u8(&mut header, &mut hpos, first).ok()?;
        put_remaining_length(&mut header, &mut hpos, remaining).ok()?;
        let header_len = hpos; // actual header length: <= MAX_HEADER, often less.

        // Shift the already-written topic+payload left from the worst-case offset (MAX_HEADER) to
        // the actual header length, closing the gap left by a shorter-than-worst-case varint. This
        // is a left shift (dst <= src), so `copy_within` is safe/correct without an extra buffer.
        let body_len = topic_field_len + payload_len;
        out.copy_within(MAX_HEADER..MAX_HEADER + body_len, header_len);
        out.get_mut(..header_len)?
            .copy_from_slice(&header[..header_len]);

        Some(header_len + body_len)
    }

    /// Housekeeping tick — reserved for future keepalive/timeout enforcement. Currently a no-op:
    /// this minimal broker doesn't track per-client keep-alive intervals (§ Scope), but the seam is
    /// here so a caller's poll loop can call it unconditionally alongside `NetStack::poll`.
    pub fn poll(&mut self, _now_ms: u64) {}
}
