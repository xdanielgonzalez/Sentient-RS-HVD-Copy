//! A minimal, caller-driven **MQTT 3.1.1 broker core**: the device hosts the server side, so
//! unmodified stock MQTT 3.1.1 clients connect *to* it (see the Phase-2 emulation plan's Design
//! Decision D1). Pure and `no_std`/no-alloc, mirroring the `feed_incoming`/`drain_outgoing` seam
//! [`crate::link::mqtt_session::MqttSession`] uses on the client side: [`BrokerCore`] does no I/O of its
//! own — the caller (a socket pump) feeds it bytes read from each client connection and drains
//! bytes to write back, per client `slot`.
//!
//! ## Memory model (render-on-demand — see the Phase-2 plan's "MEMORY-MODEL REDESIGN")
//!
//! Earlier revisions of this broker stored serialized MQTT *payload bytes*: a retained byte-store
//! plus a per-client outgoing byte buffer. On a 64 KB MCU that serialized data (often ~10x inflated
//! JSON relative to the compact in-memory reading it came from) blew the RAM budget. **This broker
//! stores no payload bytes at all.** It tracks only:
//! * which topics are *active* (published at least once) and, of those, which are *retained*
//!   (topic strings only — `ActiveTopic`);
//! * per client, a bounded list of *pending publishes* (topic strings only — no payload).
//!
//! Payload bytes are rendered on demand, one packet at a time, by a caller-provided closure passed
//! to [`BrokerCore::drain_outgoing`]. The caller (firmware, in a later step) renders its compact
//! canonical readings into TMS JSON only at this final output step, into a buffer it already owns —
//! nothing is serialized until the instant it's about to go on the wire, and nothing serialized
//! lingers afterward.
//!
//! ## Scope (deliberately minimal)
//!
//! * MQTT **3.1.1 only** (protocol name `MQTT`, level 4).
//! * CONNECT -> CONNACK, SUBSCRIBE -> SUBACK (QoS 0 granted for every filter), PUBLISH fanout at
//!   **QoS 0 only**, PINGREQ -> PINGRESP, DISCONNECT (and any malformed/short input) closes the
//!   client's slot cleanly.
//! * `#`/`+` topic-filter wildcards (MQTT 3.1.1 section 4.7), implemented as the pure
//!   [`crate::link::mqtt_server::topic_matches`] function.
//! * A bounded active-topic table: [`crate::link::mqtt_server::MAX_RETAINED`] fixed slots keyed by topic, each carrying just
//!   its retained flag. A fresh SUBSCRIBE whose filter matches a *retained* active topic enqueues
//!   that topic onto the new subscriber's pending-publish list (RETAIN flag set on render, MQTT
//!   3.1.1 section 3.3.1.3) — the actual bytes are rendered lazily by the caller's closure at drain
//!   time, not replayed from storage.
//! * A bounded client table ([`BrokerCore`]'s `MAX_CLIENTS` const generic), each with a bounded
//!   subscription-filter list and a bounded pending-publish (topic only) list. A pending-publish
//!   entry that doesn't fit is dropped (never blocks, never panics) and counted — see
//!   [`BrokerCore::dropped_count`].
//!
//! ## Explicitly out of scope
//!
//! * QoS 1/2 delivery (PUBACK/PUBREC/PUBREL/PUBCOMP) — a documented future extension; every PUBLISH
//!   this broker sends is QoS 0 regardless of what a subscriber's SUBSCRIBE requested.
//! * Persistent sessions (Clean Session is accepted but not distinguished — nothing survives a
//!   `client_closed` regardless).
//! * Will messages, authentication/TLS, retain-delete (an empty-payload retained PUBLISH is not
//!   specially treated), and wildcards other than `#`/`+`.
//!
//! Reuses the shared wire primitives in `crate::link::mqtt_wire` — the same fixed-header/Remaining
//! Length/UTF-8-string building blocks [`crate::link::mqtt_client`] encodes with — so the wire format is
//! never re-invented.

mod prelude;
mod types;

mod impl_broker_core_connect;
mod impl_broker_core_default;
mod impl_broker_core_dispatch;
mod impl_broker_core_lifecycle;
mod impl_broker_core_outgoing;
mod impl_broker_core_ping;
mod impl_broker_core_publish;
mod impl_broker_core_subscribe;
mod impl_client_slot;

pub use types::*;

/// Max concurrent clients if the caller doesn't override [`BrokerCore`]'s const generic.
const DEFAULT_MAX_CLIENTS: usize = 4;
/// Fixed active-topic slots: the device's 3 `tms/...` topics + 1 monitor topic (see the Phase-2
/// emulation plan). Not configurable — this broker is sized for exactly that fleet.
pub const MAX_RETAINED: usize = 4;
/// Max bytes of a topic string tracked by the active-topic table. This device's actual topics are
/// all well under 64 bytes (`tms/METEOR-001/active_diagnostic_state` is the longest at 39,
/// `monitor/METEOR-001/console` is 26) — 64 keeps generous headroom (unlike [`FILTER_CAP`], which
/// is an *external subscriber's* filter string, not a topic this device publishes, so it isn't
/// bounded by this device's own topic-name lengths).
const TOPIC_CAP: usize = 64;
/// Max subscription filters tracked per client.
const MAX_FILTERS_PER_CLIENT: usize = 8;
/// Max bytes of one topic filter string.
const FILTER_CAP: usize = 128;
/// Max pending-publish entries (topic strings — no payload bytes) queued per client between
/// [`BrokerCore::drain_outgoing`] calls. Each entry is just a [`TOPIC_CAP`]-byte topic string, so
/// this list is cheap even sized generously relative to the old byte-buffer world.
const MAX_PENDING_PER_CLIENT: usize = 8;
/// Inbound reassembly capacity per client: one CONNECT, or a handful of small control packets
/// (SUBSCRIBE/PINGREQ/DISCONNECT) coalesced in a single transport read.
const RX_CAP: usize = 512;
/// Max bytes of one staged CONTROL packet (CONNACK/SUBACK/PINGRESP) per client. CONNACK is 4 bytes
/// fixed; PINGRESP is 2; SUBACK is `5 + 2 + MAX_FILTERS_PER_CLIENT` (see [`handle_subscribe`]'s
/// SUBACK encode) — sized to the largest of the three with no payload bytes anywhere in this
/// buffer (this broker no longer stages PUBLISH bytes ahead of drain time at all; those are
/// rendered directly into the caller's `out` buffer by [`BrokerCore::drain_outgoing`]).
const CONTROL_CAP: usize = 5 + 2 + MAX_FILTERS_PER_CLIENT;

/// Does `topic` (a concrete published topic — no wildcards) match `filter` (a subscription filter,
/// which may contain `+`/`#`) per MQTT 3.1.1 section 4.7?
///
/// * `#` matches any number of levels *including zero*, and must be the last filter level.
/// * `+` matches exactly one level.
/// * A filter starting with `$` only matches a topic starting with the identical literal `$...`
///   segment — `#`/`+` as the *first* level never match a `$`-prefixed topic (MQTT 3.1.1 section
///   4.7.2), since `$`-topics (e.g. `$SYS/...`) are reserved and subscribers must opt in
///   explicitly. This broker publishes no `$`-topics itself, but the matcher still honors the rule
///   for interop with subscribers that probe it.
pub fn topic_matches(filter: &str, topic: &str) -> bool {
    // The `$`-exclusion rule only bites when the *filter's first level* is a wildcard matching a
    // `$`-prefixed topic; walk level by level so `$SYS/foo` vs filter `#`/`+/foo` is rejected while
    // `$SYS/foo` vs filter `$SYS/#` still matches (a literal `$SYS` first level compares equal in
    // the general per-level branch below, so it needs no special case).
    let mut filter_levels = filter.split('/');
    let mut topic_levels = topic.split('/');
    let mut first_level = true;

    loop {
        let f = filter_levels.next();
        let t = topic_levels.next();
        match (f, t) {
            (Some("#"), _) => {
                if first_level && t.is_some_and(|lvl| lvl.starts_with('$')) {
                    return false;
                }
                return true; // `#` matches the rest, including zero further levels.
            }
            (Some("+"), Some(t_lvl)) => {
                if first_level && t_lvl.starts_with('$') {
                    return false;
                }
                first_level = false;
            }
            (Some("+"), None) => return false, // `+` requires exactly one level to be present.
            (Some(f_lvl), Some(t_lvl)) => {
                if f_lvl != t_lvl {
                    return false;
                }
                first_level = false;
            }
            (Some(_), None) => return false, // filter has more levels than topic.
            (None, Some(_)) => return false, // topic has more levels than filter.
            (None, None) => return true,     // both exhausted at the same level — exact match.
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::prelude::*;
    use crate::link::mqtt_client::{encode_connect, parse_connack, ConnectOptions};

    fn connect_opts(id: &str) -> ConnectOptions<'_> {
        ConnectOptions {
            client_id: id,
            keep_alive_secs: 60,
            clean_session: true,
            username: None,
            password: None,
        }
    }

    /// Drive slot `slot` through CONNECT -> CONNACK(rc=0) using lib-link's own client encoder as
    /// the peer, asserting acceptance.
    fn connect_client<const N: usize>(broker: &mut BrokerCore<N>, slot: usize, client_id: &str) {
        broker.client_connected(slot);
        let mut buf = [0u8; 128];
        let n = encode_connect(&connect_opts(client_id), &mut buf).unwrap();
        broker.feed_incoming(slot, &buf[..n]).unwrap();
        let mut out = [0u8; 64];
        let mut render = |_: &str, _: &mut [u8]| -> Option<usize> { None };
        let n = broker.drain_outgoing(slot, &mut out, &mut render);
        let ack = parse_connack(&out[..n]).unwrap();
        assert_eq!(ack.return_code, 0, "CONNECT should be accepted");
    }

    /// Encode a bare-bones SUBSCRIBE packet (one filter, requested QoS 0) using the shared wire
    /// primitives directly (mqtt_client.rs has no SUBSCRIBE encoder — this broker is the only
    /// consumer of that packet type today, so the test builds it from the same primitives the
    /// broker parses with, not hand-rolled bytes).
    fn encode_subscribe(packet_id: u16, filter: &str) -> heapless::Vec<u8, 256> {
        let mut buf = [0u8; 256];
        let mut pos = 0usize;
        let remaining = 2 + 2 + filter.len() + 1;
        put_u8(&mut buf, &mut pos, 0x82).unwrap(); // SUBSCRIBE, reserved flags 0010
        put_remaining_length(&mut buf, &mut pos, remaining).unwrap();
        put_u16(&mut buf, &mut pos, packet_id).unwrap();
        put_str_field(&mut buf, &mut pos, filter.as_bytes()).unwrap();
        put_u8(&mut buf, &mut pos, 0x00).unwrap(); // requested QoS 0
        heapless::Vec::from_slice(&buf[..pos]).unwrap()
    }

    /// A render closure backed by a small fixture table (`(topic, payload)` pairs), for tests that
    /// want deterministic, known bytes per topic — mirrors what the firmware's real renderer will
    /// do (map canonical reading -> TMS JSON), without any of that machinery.
    fn fixture_render<'a>(
        fixtures: &'a [(&'a str, &'a [u8])],
    ) -> impl FnMut(&str, &mut [u8]) -> Option<usize> + 'a {
        move |topic, buf| {
            let (_, payload) = fixtures.iter().find(|(t, _)| *t == topic)?;
            if payload.len() > buf.len() {
                return None;
            }
            buf[..payload.len()].copy_from_slice(payload);
            Some(payload.len())
        }
    }

    /// Drain every packet currently available for `slot` (control frames + one PUBLISH per pending
    /// entry), returning each as an owned byte vec, using `render` for payload bytes.
    fn drain_all<const N: usize, R: FnMut(&str, &mut [u8]) -> Option<usize>>(
        broker: &mut BrokerCore<N>,
        slot: usize,
        render: &mut R,
    ) -> heapless::Vec<heapless::Vec<u8, 2048>, 16> {
        let mut packets = heapless::Vec::new();
        loop {
            let mut out = [0u8; 2048];
            let n = broker.drain_outgoing(slot, &mut out, render);
            if n == 0 {
                break;
            }
            let _ = packets.push(heapless::Vec::from_slice(&out[..n]).unwrap());
        }
        packets
    }

    // -- topic_matches: exhaustive per MQTT 3.1.1 section 4.7 --------------------------------

    #[test]
    fn exact_match() {
        assert!(topic_matches(
            "sport/tennis/player1",
            "sport/tennis/player1"
        ));
        assert!(!topic_matches(
            "sport/tennis/player1",
            "sport/tennis/player2"
        ));
    }

    #[test]
    fn multi_level_wildcard_matches_all_sublevels_and_own_level() {
        assert!(topic_matches("sport/#", "sport/tennis/player1"));
        assert!(topic_matches("sport/#", "sport/tennis/player1/ranking"));
        assert!(topic_matches("sport/#", "sport")); // `#` matches zero further levels too.
    }

    #[test]
    fn bare_hash_matches_everything_except_dollar_topics() {
        assert!(topic_matches("#", "sport/tennis/player1"));
        assert!(topic_matches("#", "a"));
        assert!(!topic_matches("#", "$SYS/broker/uptime"));
    }

    #[test]
    fn single_level_wildcard() {
        assert!(topic_matches("sport/+/player1", "sport/tennis/player1"));
        assert!(!topic_matches(
            "sport/+/player1",
            "sport/tennis/player1/ranking"
        ));
        assert!(topic_matches("sport/+", "sport/"));
        assert!(topic_matches("+/+", "sport/tennis"));
        assert!(topic_matches("+", "sport"));
        assert!(!topic_matches("+", "sport/tennis")); // + is exactly one level.
    }

    #[test]
    fn plus_does_not_match_dollar_prefixed_first_level() {
        assert!(!topic_matches("+/monitor", "$SYS/monitor"));
        assert!(topic_matches("$SYS/+", "$SYS/monitor")); // literal $SYS first level still fine.
    }

    #[test]
    fn trailing_slash_edge_cases() {
        // "sport/" has a trailing empty level; a filter must match it explicitly.
        assert!(topic_matches("sport/", "sport/"));
        assert!(!topic_matches("sport", "sport/"));
        assert!(!topic_matches("sport/", "sport"));
        assert!(topic_matches("sport/#", "sport/"));
    }

    #[test]
    fn filter_and_topic_level_count_mismatch_never_matches_without_wildcard() {
        assert!(!topic_matches("a/b", "a/b/c"));
        assert!(!topic_matches("a/b/c", "a/b"));
    }

    // -- CONNECT / CONNACK -----------------------------------------------------------------

    #[test]
    fn connect_with_valid_protocol_is_accepted() {
        let mut broker: BrokerCore<4> = BrokerCore::new();
        connect_client(&mut broker, 0, "dev01");
    }

    #[test]
    fn connect_with_wrong_protocol_level_is_refused_and_closes_slot() {
        let mut broker: BrokerCore<4> = BrokerCore::new();
        broker.client_connected(0);
        // Hand-build a CONNECT with protocol level 3 (MQIsdp-style / pre-3.1.1) — this broker only
        // speaks 3.1.1 (level 4).
        let mut buf = [0u8; 64];
        let mut pos = 0usize;
        let remaining = 10 + 2 + 5; // "MQTT" str field(6) + level+flags+keepalive(4) + client id "dev01"
        put_u8(&mut buf, &mut pos, 0x10).unwrap();
        put_remaining_length(&mut buf, &mut pos, remaining).unwrap();
        put_str_field(&mut buf, &mut pos, b"MQTT").unwrap();
        put_u8(&mut buf, &mut pos, 0x03).unwrap(); // wrong level
        put_u8(&mut buf, &mut pos, 0x02).unwrap();
        put_u16(&mut buf, &mut pos, 60).unwrap();
        put_str_field(&mut buf, &mut pos, b"dev01").unwrap();

        let err = broker.feed_incoming(0, &buf[..pos]);
        assert_eq!(err, Err(BrokerError::UnsupportedProtocol));

        let mut out = [0u8; 16];
        let mut render = |_: &str, _: &mut [u8]| -> Option<usize> { None };
        let n = broker.drain_outgoing(0, &mut out, &mut render);
        let ack = parse_connack(&out[..n]).unwrap();
        assert_eq!(
            ack.return_code, 1,
            "refusal code = unacceptable protocol version"
        );
    }

    // -- SUBSCRIBE / SUBACK -----------------------------------------------------------------

    #[test]
    fn subscribe_grants_qos0_for_every_filter() {
        let mut broker: BrokerCore<4> = BrokerCore::new();
        connect_client(&mut broker, 0, "dev01");
        let sub = encode_subscribe(7, "tms/#");
        broker.feed_incoming(0, &sub).unwrap();
        let mut out = [0u8; 64];
        let mut render = |_: &str, _: &mut [u8]| -> Option<usize> { None };
        let n = broker.drain_outgoing(0, &mut out, &mut render);
        assert!(
            n >= 5,
            "SUBACK must be at least fixed-header(2)+packet-id(2)+1 grant"
        );
        assert_eq!(out[0], 0x90, "SUBACK packet type");
        assert_eq!(u16::from_be_bytes([out[2], out[3]]), 7, "echoes packet id");
        assert_eq!(out[4], 0x00, "grants QoS 0");
    }

    // -- Rendered retained replay to a late subscriber ---------------------------------------

    #[test]
    fn retained_topic_replays_via_render_to_a_late_subscriber_with_retain_flag_set() {
        const TOPIC: &str = "tms/METEOR-001/dc_measurement_update";
        let fixtures: [(&str, &[u8]); 1] = [(TOPIC, b"{\"v\":42}")];

        let mut broker: BrokerCore<4> = BrokerCore::new();
        // Publish (retained) before anyone subscribes — no payload passed, topic-only.
        broker.publish_topic(TOPIC, true);

        connect_client(&mut broker, 0, "late-sub");
        broker
            .feed_incoming(0, &encode_subscribe(9, "tms/#"))
            .unwrap();

        let mut render = fixture_render(&fixtures);
        let packets = drain_all(&mut broker, 0, &mut render);
        // Expect SUBACK first, then the retained PUBLISH.
        assert_eq!(packets.len(), 2, "SUBACK + one retained PUBLISH");
        assert_eq!(packets[0][0], 0x90, "SUBACK first");
        let publish = &packets[1];
        assert_eq!(publish[0] & 0xF0, 0x30, "PUBLISH packet type");
        assert_eq!(publish[0] & 0x01, 0x01, "RETAIN flag set on replay");
        assert!(
            publish.ends_with(fixtures[0].1),
            "replayed payload matches what the render closure produced for this topic"
        );
    }

    // -- Live publish fanout to multiple clients, rendered per-recipient ---------------------

    #[test]
    fn live_publish_fans_out_to_all_matching_subscribers_rendered() {
        const TOPIC: &str = "tms/METEOR-001/dc_measurement_update";
        let fixtures: [(&str, &[u8]); 1] = [(TOPIC, b"{\"v\":1}")];

        let mut broker: BrokerCore<4> = BrokerCore::new();
        connect_client(&mut broker, 0, "sub-a");
        connect_client(&mut broker, 1, "sub-b");
        broker
            .feed_incoming(0, &encode_subscribe(1, "tms/#"))
            .unwrap();
        broker
            .feed_incoming(1, &encode_subscribe(1, "tms/#"))
            .unwrap();
        // Drain the SUBACKs (and any retained replay — none yet) before the live publish.
        let mut no_render = |_: &str, _: &mut [u8]| -> Option<usize> { None };
        drain_all(&mut broker, 0, &mut no_render);
        drain_all(&mut broker, 1, &mut no_render);

        broker.publish_topic(TOPIC, false);

        for slot in [0usize, 1] {
            let mut render = fixture_render(&fixtures);
            let packets = drain_all(&mut broker, slot, &mut render);
            assert_eq!(
                packets.len(),
                1,
                "slot {slot} should receive exactly the fanned-out PUBLISH"
            );
            let out = &packets[0];
            assert_eq!(out[0] & 0xF0, 0x30, "PUBLISH packet type");
            assert_eq!(out[0] & 0x01, 0x00, "not retained (live publish)");
            assert!(
                out.ends_with(fixtures[0].1),
                "rendered payload reaches the subscriber"
            );
        }
    }

    #[test]
    fn non_matching_subscriber_receives_nothing() {
        let mut broker: BrokerCore<4> = BrokerCore::new();
        connect_client(&mut broker, 0, "sub-a");
        broker
            .feed_incoming(0, &encode_subscribe(1, "other/#"))
            .unwrap();
        let mut no_render = |_: &str, _: &mut [u8]| -> Option<usize> { None };
        drain_all(&mut broker, 0, &mut no_render);

        broker.publish_topic("tms/METEOR-001/dc_measurement_update", false);
        let mut out = [0u8; 256];
        let mut render = |_: &str, _: &mut [u8]| -> Option<usize> { None };
        let n = broker.drain_outgoing(0, &mut out, &mut render);
        assert_eq!(n, 0);
    }

    #[test]
    fn retained_publish_replaces_previous_value_for_same_topic() {
        const TOPIC: &str = "monitor/METEOR-001/console";
        let mut broker: BrokerCore<4> = BrokerCore::new();
        broker.publish_topic(TOPIC, true);
        broker.publish_topic(TOPIC, true);

        connect_client(&mut broker, 0, "late-sub");
        broker
            .feed_incoming(0, &encode_subscribe(1, "monitor/#"))
            .unwrap();

        // The active-topic table tracks only the topic + retained flag (no value to "replace" any
        // more) — the render closure decides what bytes come back "second" is what's live now.
        let fixtures: [(&str, &[u8]); 1] = [(TOPIC, b"second")];
        let mut render = fixture_render(&fixtures);
        let packets = drain_all(&mut broker, 0, &mut render);
        // SUBACK + exactly one retained PUBLISH (re-publishing the same topic doesn't create a
        // second active-topic entry, and re-subscribing to an already-pending topic doesn't
        // double-queue it).
        assert_eq!(packets.len(), 2);
        assert!(packets[1].ends_with(b"second"));
    }

    // -- Large rendered payload frames correctly ----------------------------------------------

    #[test]
    fn large_rendered_payload_frames_correctly_with_remaining_length_varint() {
        const TOPIC: &str = "tms/METEOR-001/power_hardware_update";
        const PAYLOAD_LEN: usize = 1553; // real PowerHardwareUpdate JSON size (T3 golden fixture).
        let payload = [0xABu8; PAYLOAD_LEN];
        let fixtures: [(&str, &[u8]); 1] = [(TOPIC, &payload)];

        let mut broker: BrokerCore<4> = BrokerCore::new();
        connect_client(&mut broker, 0, "sub");
        broker
            .feed_incoming(0, &encode_subscribe(1, "tms/#"))
            .unwrap();
        let mut no_render = |_: &str, _: &mut [u8]| -> Option<usize> { None };
        drain_all(&mut broker, 0, &mut no_render); // drain SUBACK

        broker.publish_topic(TOPIC, false);

        let min_len = BrokerCore::<4>::min_out_len(PAYLOAD_LEN);
        let mut out = vec![0u8; min_len];
        let mut render = fixture_render(&fixtures);
        let n = broker.drain_outgoing(0, &mut out, &mut render);
        assert!(
            n > 0,
            "large payload must still frame into a large-enough out buffer"
        );

        // Remaining Length must decode to topic_field_len + payload_len, and the packet must end
        // exactly at n with the full payload intact.
        let header = parse_fixed_header(&out[..n]).unwrap();
        assert_eq!(header.packet_type, PKT_PUBLISH);
        let expected_remaining = 2 + TOPIC.len() + PAYLOAD_LEN;
        assert_eq!(header.remaining_length, expected_remaining);
        assert_eq!(header.header_len + header.remaining_length, n);
        assert!(
            out[..n].ends_with(&payload[..]),
            "full rendered payload present, unmangled"
        );
    }

    // -- Guard-byte fuzz: render-into-`out` framing never writes past `out`'s bounds ----------
    //
    // Host oracle for the "intermittent broker-thread corruption with ASCII-digit-bytes-in-PC"
    // fault (see the Phase-2 plan's render/output-path investigation): places `out` inside a
    // larger allocation flanked by canary bytes immediately before/after, drives
    // `render_publish`/`drain_outgoing` with rendered payloads of every size from 0 up through
    // (and past) `out`'s capacity — including the Remaining-Length varint width boundaries at
    // 127/128 and 16383/16384 — and asserts after every single call that (a) the canaries are
    // untouched and (b) the returned length never exceeds the real `out` slice. Run under Miri
    // (`cargo +nightly miri test -p lib-link guard_bytes`) to additionally catch `&mut`
    // aliasing/provenance UB deterministically, not just a length overrun.
    mod guard_byte_fuzz {
        use super::*;

        const GUARD_LEN: usize = 64;
        const GUARD_BYTE: u8 = 0xA5; // canary pattern unlikely to appear in real frame bytes.

        /// One allocation shaped `[guard][out][guard]`; `out_range()` gives the real `out` window,
        /// `assert_guards_intact()` checks both canary regions are byte-for-byte untouched.
        struct GuardedBuf {
            backing: Vec<u8>,
            out_len: usize,
        }

        impl GuardedBuf {
            fn new(out_len: usize) -> Self {
                let mut backing = vec![GUARD_BYTE; GUARD_LEN + out_len + GUARD_LEN];
                // Fill the real `out` region with a distinct non-canary pattern so a short write
                // leaving stale canary-colored bytes inside `out` doesn't masquerade as fine.
                for b in &mut backing[GUARD_LEN..GUARD_LEN + out_len] {
                    *b = 0x00;
                }
                GuardedBuf { backing, out_len }
            }

            fn out_mut(&mut self) -> &mut [u8] {
                &mut self.backing[GUARD_LEN..GUARD_LEN + self.out_len]
            }

            /// Panics with a precise pinpoint (which guard, which byte offset) if any canary byte
            /// was clobbered — the host-checker's "definitive failure" signal.
            fn assert_guards_intact(&self, ctx: &str) {
                for (i, &b) in self.backing[..GUARD_LEN].iter().enumerate() {
                    assert_eq!(
                        b, GUARD_BYTE,
                        "LEFT guard byte {i} clobbered ({ctx}): overflow wrote BEFORE `out`"
                    );
                }
                let right_start = GUARD_LEN + self.out_len;
                for (i, &b) in self.backing[right_start..].iter().enumerate() {
                    assert_eq!(
                        b, GUARD_BYTE,
                        "RIGHT guard byte {i} clobbered ({ctx}): overflow wrote AFTER `out` \
                         (out_len={}, this is the render-framing OOB write)",
                        self.out_len
                    );
                }
            }
        }

        /// Drive one `drain_outgoing` call with a render closure that emits exactly `payload_len`
        /// bytes of a recognizable non-zero pattern, then assert guards are intact and the
        /// returned length is sane. Returns the drained length (0 if nothing was produced, e.g.
        /// because `out` was too small to hold the frame at all — never a bug by itself).
        fn drive_one(topic: &str, out_len: usize, payload_len: usize) -> usize {
            let mut broker: BrokerCore<4> = BrokerCore::new();
            connect_client(&mut broker, 0, "fuzz");
            broker.feed_incoming(0, &encode_subscribe(1, "#")).unwrap();
            let mut no_render = |_: &str, _: &mut [u8]| -> Option<usize> { None };
            drain_all(&mut broker, 0, &mut no_render); // drain SUBACK first.

            broker.publish_topic(topic, false);

            let mut guarded = GuardedBuf::new(out_len);
            let mut render = |_: &str, buf: &mut [u8]| -> Option<usize> {
                if payload_len > buf.len() {
                    return None; // mirrors a real renderer: never claim more than it was offered.
                }
                for (i, b) in buf[..payload_len].iter_mut().enumerate() {
                    *b = (i % 251) as u8 + 1; // non-zero, non-canary recognizable fill.
                }
                Some(payload_len)
            };
            let n = broker.drain_outgoing(0, guarded.out_mut(), &mut render);

            guarded.assert_guards_intact(&format!(
                "topic={topic:?} out_len={out_len} payload_len={payload_len}"
            ));
            assert!(
                n <= out_len,
                "drain_outgoing returned n={n} > out_len={out_len} \
                 (topic={topic:?} payload_len={payload_len}) — length lies about what was written"
            );
            n
        }

        #[test]
        fn every_payload_size_from_zero_through_max_payload_stays_in_bounds() {
            const TOPIC: &str = "tms/METEOR-001/power_hardware_update";
            // MAX_PAYLOAD (lib-mapping) is 2048 (this literal tracks it — lib-net cannot depend on
            // lib-mapping); exercise well past it too, since `render_publish` must reject (via
            // `payload_off.checked_add(payload_len)? > out.len()`) rather than trust a misbehaving
            // renderer that over-reports its own bounds.
            const OUT_LEN: usize = BrokerCore::<4>::min_out_len(2048);
            for payload_len in 0..=OUT_LEN {
                drive_one(TOPIC, OUT_LEN, payload_len);
            }
        }

        #[test]
        fn varint_width_boundaries_127_128_16383_16384_stay_in_bounds() {
            const TOPIC: &str = "tms/METEOR-001/dc_measurement_update";
            // Remaining Length = topic_field_len + payload_len; topic_field_len for this topic is
            // 2 + 37 = 39. Pick payload lengths that land `remaining` exactly on/around the 1-byte
            // <-> 2-byte (127/128) and 2-byte <-> 3-byte (16383/16384) varint-width boundaries.
            let topic_field_len = 2 + TOPIC.len();
            let boundary_remaining: [usize; 8] = [126, 127, 128, 129, 16382, 16383, 16384, 16385];
            for remaining in boundary_remaining {
                let Some(payload_len) = remaining.checked_sub(topic_field_len) else {
                    continue;
                };
                let out_len = BrokerCore::<4>::min_out_len(payload_len);
                drive_one(TOPIC, out_len, payload_len);
            }
        }

        #[test]
        fn out_exactly_min_len_and_one_under_and_one_over_stay_in_bounds() {
            const TOPIC: &str = "monitor/METEOR-001/console";
            const PAYLOAD_LEN: usize = 256;
            let exact = BrokerCore::<4>::min_out_len(PAYLOAD_LEN);
            for out_len in [exact.saturating_sub(1), exact, exact + 1, exact + 64] {
                drive_one(TOPIC, out_len, PAYLOAD_LEN);
            }
        }

        #[test]
        fn out_smaller_than_topic_reservation_never_touches_out_at_all() {
            // `out` too small even for the worst-case header+topic reservation: `render_publish`
            // must bail before writing anything (checked at `payload_off > out.len()`).
            const TOPIC: &str = "tms/METEOR-001/active_diagnostic_state"; // 39-byte topic.
            for out_len in 0..8 {
                let n = drive_one(TOPIC, out_len, 0);
                assert_eq!(
                    n, 0,
                    "undersized out must yield 0, never a truncated/garbage frame"
                );
            }
        }

        #[test]
        fn render_over_reporting_its_own_payload_len_is_rejected_not_trusted() {
            // A hostile/buggy renderer that writes into `buf` correctly but then *lies* about how
            // many bytes it wrote (claims more than `buf.len()`) must be rejected by
            // `render_publish`'s own `payload_off.checked_add(payload_len)? > out.len()` guard,
            // not trusted into an OOB `copy_within`.
            const TOPIC: &str = "tms/METEOR-001/dc_measurement_update";
            const OUT_LEN: usize = 128;
            let mut broker: BrokerCore<4> = BrokerCore::new();
            connect_client(&mut broker, 0, "fuzz");
            broker.feed_incoming(0, &encode_subscribe(1, "#")).unwrap();
            let mut no_render = |_: &str, _: &mut [u8]| -> Option<usize> { None };
            drain_all(&mut broker, 0, &mut no_render);
            broker.publish_topic(TOPIC, false);

            let mut guarded = GuardedBuf::new(OUT_LEN);
            // Writes only 4 real bytes but claims the full buffer length + 1 — impossible for a
            // well-behaved renderer, but `render_publish` must not trust the claim blindly.
            let mut render = |_: &str, buf: &mut [u8]| -> Option<usize> {
                let claim = buf.len() + 1;
                buf[..4].copy_from_slice(&[1, 2, 3, 4]);
                Some(claim)
            };
            let n = broker.drain_outgoing(0, guarded.out_mut(), &mut render);
            guarded.assert_guards_intact("over-reporting renderer");
            assert_eq!(
                n, 0,
                "an over-reporting renderer's frame must be rejected (n=0), not framed"
            );
        }
    }

    // -- Pending-publish overflow drops + counts, never panics --------------------------------

    #[test]
    fn pending_publish_overflow_drops_and_counts_rather_than_panicking() {
        let mut broker: BrokerCore<4> = BrokerCore::new();
        connect_client(&mut broker, 0, "dev01");
        broker
            .feed_incoming(0, &encode_subscribe(1, "big/#"))
            .unwrap();
        let mut no_render = |_: &str, _: &mut [u8]| -> Option<usize> { None };
        drain_all(&mut broker, 0, &mut no_render); // drain SUBACK

        assert_eq!(broker.dropped_count(), 0);
        // Publish enough distinct topics, without ever draining, to overflow the per-client
        // pending-publish list (MAX_PENDING_PER_CLIENT = 8) — `push_pending` must drop rather than
        // panic/block once full. Distinct topic names avoid the active-topic table's own 4-slot cap
        // masking the pending-list overflow (topics beyond MAX_RETAINED simply aren't tracked as
        // active/retained, but fan-out to already-subscribed clients still enqueues a pending entry
        // for each `publish_topic` call).
        for i in 0..16 {
            let mut topic: heapless::String<32> = heapless::String::new();
            use core::fmt::Write;
            write!(topic, "big/topic{i}").unwrap();
            broker.publish_topic(&topic, false);
        }
        assert!(broker.dropped_count() > 0, "overflow must be counted");
    }

    // -- `#`/`+` matching matrix (fan-out gating) ----------------------------------------------

    #[test]
    fn wildcard_matching_matrix_gates_fanout_correctly() {
        let mut broker: BrokerCore<4> = BrokerCore::new();
        connect_client(&mut broker, 0, "hash-sub");
        connect_client(&mut broker, 1, "plus-sub");
        connect_client(&mut broker, 2, "exact-sub");
        broker
            .feed_incoming(0, &encode_subscribe(1, "tms/#"))
            .unwrap();
        broker
            .feed_incoming(1, &encode_subscribe(1, "tms/+/dc_measurement_update"))
            .unwrap();
        broker
            .feed_incoming(
                2,
                &encode_subscribe(1, "tms/METEOR-001/power_hardware_update"),
            )
            .unwrap();
        let mut no_render = |_: &str, _: &mut [u8]| -> Option<usize> { None };
        for slot in 0..3 {
            drain_all(&mut broker, slot, &mut no_render);
        }

        broker.publish_topic("tms/METEOR-001/dc_measurement_update", false);

        let fixtures: [(&str, &[u8]); 1] = [("tms/METEOR-001/dc_measurement_update", b"x")];
        let mut r0 = fixture_render(&fixtures);
        let mut r1 = fixture_render(&fixtures);
        let mut r2 = fixture_render(&fixtures);
        assert_eq!(drain_all(&mut broker, 0, &mut r0).len(), 1, "# matches");
        assert_eq!(
            drain_all(&mut broker, 1, &mut r1).len(),
            1,
            "+ matches this level shape"
        );
        assert_eq!(
            drain_all(&mut broker, 2, &mut r2).len(),
            0,
            "exact filter for a different topic must not match"
        );
    }

    // -- PING ---------------------------------------------------------------------------------

    #[test]
    fn pingreq_gets_pingresp() {
        let mut broker: BrokerCore<4> = BrokerCore::new();
        connect_client(&mut broker, 0, "dev01");
        broker.feed_incoming(0, &[0xC0, 0x00]).unwrap();
        let mut out = [0u8; 8];
        let mut render = |_: &str, _: &mut [u8]| -> Option<usize> { None };
        let n = broker.drain_outgoing(0, &mut out, &mut render);
        assert_eq!(&out[..n], &[0xD0, 0x00]);
    }

    // -- DISCONNECT / malformed input closes the slot cleanly, never panics ------------------

    #[test]
    fn disconnect_closes_slot_without_error() {
        let mut broker: BrokerCore<4> = BrokerCore::new();
        connect_client(&mut broker, 0, "dev01");
        broker.feed_incoming(0, &[0xE0, 0x00]).unwrap();
        // Slot is closed: a further publish that would have matched a prior subscription does
        // nothing (no panic, no output).
        broker.publish_topic("tms/x", false);
        let mut out = [0u8; 16];
        let mut render = |_: &str, _: &mut [u8]| -> Option<usize> { None };
        assert_eq!(broker.drain_outgoing(0, &mut out, &mut render), 0);
    }

    #[test]
    fn malformed_bytes_close_the_slot_without_panicking() {
        let mut broker: BrokerCore<4> = BrokerCore::new();
        broker.client_connected(0);
        let garbage = [0xFFu8; 32];
        let err = broker.feed_incoming(0, &garbage);
        assert!(err.is_err());
        // Slot is now closed; feeding more bytes reports InvalidSlot rather than panicking.
        assert_eq!(
            broker.feed_incoming(0, &[0x00]),
            Err(BrokerError::InvalidSlot)
        );
    }

    #[test]
    fn oversized_remaining_length_is_rejected_not_panicked() {
        let mut broker: BrokerCore<4> = BrokerCore::new();
        broker.client_connected(0);
        // CONNECT type byte, then 5 continuation-bit-set varint bytes (exceeds the 4-byte max).
        let bytes = [0x10u8, 0x80, 0x80, 0x80, 0x80, 0x01];
        let err = broker.feed_incoming(0, &bytes);
        assert!(err.is_err());
    }

    #[test]
    fn feed_incoming_on_unknown_slot_is_reported_not_panicked() {
        let mut broker: BrokerCore<4> = BrokerCore::new();
        assert_eq!(
            broker.feed_incoming(0, &[0x00]),
            Err(BrokerError::InvalidSlot)
        );
        assert_eq!(
            broker.feed_incoming(99, &[0x00]),
            Err(BrokerError::InvalidSlot)
        );
    }
}
