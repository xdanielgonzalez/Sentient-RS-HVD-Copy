//! Host test matrix: drives [`crate::link::mqtt_server::BrokerCore`] through a pump loop that is
//! generic over [`super::BrokerListener`]/[`super::BrokerConn`] — a byte-for-byte mirror of
//! `bin/zephyr/platform/src/broker_pump.rs`'s `rust_broker_main` socket loop, but programmed against the
//! trait seam instead of `sz_sock_*`. Exercises the mock's constrained-transport fault matrix
//! (short writes, a small simulated window, peer reset mid-send, orderly close, WouldBlock) to
//! prove the seam's contract is sufficient for the pump's real semantics, entirely on the host —
//! no emulator required.

use super::{
    BrokerConn, BrokerListener, MockBrokerConn, MockBrokerListener, MockRecvEvent, RecvOutcome,
    SendFault, SendFaultKind, SendOutcome, TcpBrokerListener,
};
use crate::link::mqtt_client::{encode_connect, parse_connack, ConnectOptions};
use crate::link::mqtt_server::BrokerCore;
use crate::link::mqtt_wire::{put_remaining_length, put_str_field, put_u16, put_u8};

const MAX_CLIENTS: usize = 4;

fn connect_opts(id: &str) -> ConnectOptions<'_> {
    ConnectOptions {
        client_id: id,
        keep_alive_secs: 60,
        clean_session: true,
        username: None,
        password: None,
    }
}

fn encode_connect_packet(client_id: &str) -> Vec<u8> {
    let mut buf = [0u8; 128];
    let n = encode_connect(&connect_opts(client_id), &mut buf).unwrap();
    buf[..n].to_vec()
}

fn encode_subscribe(packet_id: u16, filter: &str) -> Vec<u8> {
    let mut buf = [0u8; 256];
    let mut pos = 0usize;
    let remaining = 2 + 2 + filter.len() + 1;
    put_u8(&mut buf, &mut pos, 0x82).unwrap();
    put_remaining_length(&mut buf, &mut pos, remaining).unwrap();
    put_u16(&mut buf, &mut pos, packet_id).unwrap();
    put_str_field(&mut buf, &mut pos, filter.as_bytes()).unwrap();
    put_u8(&mut buf, &mut pos, 0x00).unwrap();
    buf[..pos].to_vec()
}

/// One client pump-slot, generic over the connection type — mirrors
/// `rust_broker_main`'s `ClientFd`.
struct Slot<C: BrokerConn> {
    conn: Option<C>,
}

/// A byte-for-byte port of `rust_broker_main`'s per-tick body (accept-one, per-slot
/// recv/feed/drain/send, close-on-desync), generic over `L: BrokerListener` — proves the trait
/// seam expresses everything the real pump needs, independent of which impl (Zephyr/TCP/mock)
/// backs it.
fn pump_tick<L: BrokerListener, R: FnMut(&str, &mut [u8]) -> Option<usize>>(
    listener: &mut L,
    broker: &mut BrokerCore<MAX_CLIENTS>,
    slots: &mut [Slot<L::Conn>; MAX_CLIENTS],
    render: &mut R,
) {
    if let Some(conn) = listener.accept() {
        if let Some(idx) = slots.iter().position(|s| s.conn.is_none()) {
            slots[idx].conn = Some(conn);
            broker.client_connected(idx);
        }
        // No free slot: the accepted `conn` is simply dropped (fail-closed), matching
        // `rust_broker_main`'s `sz_sock_close` refusal branch.
    }

    for (idx, slot) in slots.iter_mut().enumerate() {
        let Some(conn) = slot.conn.as_mut() else {
            continue;
        };

        let mut buf = [0u8; 2048];
        match conn.recv(&mut buf) {
            RecvOutcome::Closed => {
                broker.client_closed(idx);
                conn.close();
                slot.conn = None;
                continue;
            }
            RecvOutcome::Bytes(n) => {
                if broker.feed_incoming(idx, &buf[..n]).is_err() {
                    // BrokerCore already dooms/stages internally on Malformed/UnsupportedProtocol
                    // (drain below still flushes any staged refusal); any other error closes.
                }
            }
            RecvOutcome::WouldBlock => {}
        }

        loop {
            let mut out = [0u8; 2048];
            let n = broker.drain_outgoing(idx, &mut out, render);
            if n == 0 {
                break;
            }
            match conn.send_all(&out[..n]) {
                SendOutcome::Sent => {}
                SendOutcome::PeerGone => {
                    broker.client_closed(idx);
                    conn.close();
                    slot.conn = None;
                    break;
                }
            }
        }
    }
}

fn no_render(_: &str, _: &mut [u8]) -> Option<usize> {
    None
}

fn new_slots() -> [Slot<MockBrokerConn>; MAX_CLIENTS] {
    std::array::from_fn(|_| Slot { conn: None })
}

// -- trait object-safety / generic-dispatch sanity (no dyn anywhere) ------------------------

#[test]
fn tcp_and_mock_both_satisfy_the_same_generic_bound() {
    fn assert_listener<L: BrokerListener>() {}
    assert_listener::<TcpBrokerListener>();
    assert_listener::<MockBrokerListener>();
}

// -- case: full write (no window constraint) -------------------------------------------------

#[test]
fn full_write_connects_and_acks() {
    let mut listener = MockBrokerListener::new();
    let conn = MockBrokerConn::new(vec![MockRecvEvent::Bytes(encode_connect_packet("dev01"))]);
    listener.queue_connection(conn);

    let mut broker: BrokerCore<MAX_CLIENTS> = BrokerCore::new();
    let mut slots = new_slots();

    pump_tick(&mut listener, &mut broker, &mut slots, &mut no_render);
    let sent = &slots[0].conn.as_ref().unwrap().sent;
    let ack = parse_connack(sent).unwrap();
    assert_eq!(ack.return_code, 0);
}

// -- case: legal short-write resume (partial sends reassemble) -------------------------------

#[test]
fn short_write_resume_reassembles_full_connack() {
    let mut listener = MockBrokerListener::new();
    // Force the mock's send window to 1 byte at a time — every send_all call must internally
    // resume across many short writes.
    let conn = MockBrokerConn::new(vec![MockRecvEvent::Bytes(encode_connect_packet("dev01"))])
        .with_send_window(1);
    listener.queue_connection(conn);

    let mut broker: BrokerCore<MAX_CLIENTS> = BrokerCore::new();
    let mut slots = new_slots();

    pump_tick(&mut listener, &mut broker, &mut slots, &mut no_render);
    let sent = &slots[0].conn.as_ref().unwrap().sent;
    // CONNACK is exactly 4 bytes; byte-exact reassembly across 4 single-byte "writes" proves the
    // resume logic, not just that *some* bytes arrived.
    assert_eq!(sent.len(), 4);
    let ack = parse_connack(sent).unwrap();
    assert_eq!(ack.return_code, 0);
}

// -- case: large (~1.6 KB) payload streamed across a small simulated window ------------------

#[test]
fn large_payload_streamed_across_small_window_is_byte_exact() {
    const PAYLOAD_LEN: usize = 1600;
    let payload: Vec<u8> = (0..PAYLOAD_LEN).map(|i| (i % 251) as u8).collect();
    let payload_static: &'static [u8] = Box::leak(payload.clone().into_boxed_slice());

    let mut listener = MockBrokerListener::new();
    let conn = MockBrokerConn::new(vec![
        MockRecvEvent::Bytes(encode_connect_packet("dev01")),
        MockRecvEvent::Bytes(encode_subscribe(1, "tms/dev01/big")),
    ])
    // A window well below the packet size forces many resumed short writes across one
    // drain_outgoing-rendered PUBLISH.
    .with_send_window(64);
    listener.queue_connection(conn);

    let mut broker: BrokerCore<MAX_CLIENTS> = BrokerCore::new();
    let mut slots = new_slots();

    // `pump_tick` does exactly one `conn.recv()` per slot per call (mirroring one nonblocking
    // recv per tick in `rust_broker_main`), so the two scripted recv events — CONNECT and
    // SUBSCRIBE — are consumed on two SEPARATE ticks, not coalesced into one:
    // Tick 1: CONNECT -> CONNACK.
    // Tick 2: SUBSCRIBE -> SUBACK (registers the "tms/dev01/big" filter).
    // The client must actually be subscribed before `publish_topic` below, or `fan_out` finds no
    // matching filter and the PUBLISH is never enqueued for this slot at all.
    pump_tick(&mut listener, &mut broker, &mut slots, &mut no_render);
    pump_tick(&mut listener, &mut broker, &mut slots, &mut no_render);
    let after_control = slots[0].conn.as_ref().unwrap().sent.len();

    // Publish the big topic, then tick again so drain_outgoing renders + sends the PUBLISH.
    broker.publish_topic("tms/dev01/big", false);
    let mut render = |topic: &str, out: &mut [u8]| -> Option<usize> {
        if topic == "tms/dev01/big" {
            out.get_mut(..PAYLOAD_LEN)?.copy_from_slice(payload_static);
            Some(PAYLOAD_LEN)
        } else {
            None
        }
    };
    // Tick 3: no new accept and no new recv bytes — WouldBlock is the steady-state mock default
    // once its recv script is exhausted (both scripted events were consumed by ticks 1 and 2).
    pump_tick(&mut listener, &mut broker, &mut slots, &mut render);

    let sent = &slots[0].conn.as_ref().unwrap().sent;
    let publish_bytes = &sent[after_control..];
    // The PUBLISH payload is the tail of the framed packet — assert the exact bytes that arrived
    // over the constrained window match what was rendered, proving no desync/corruption across
    // many resumed short writes.
    assert!(publish_bytes.len() >= PAYLOAD_LEN);
    assert_eq!(
        &publish_bytes[publish_bytes.len() - PAYLOAD_LEN..],
        &payload[..]
    );
}

// -- case: peer reset mid-send -> slot closed -------------------------------------------------

#[test]
fn peer_reset_mid_send_closes_slot() {
    let mut listener = MockBrokerListener::new();
    let conn = MockBrokerConn::new(vec![MockRecvEvent::Bytes(encode_connect_packet("dev01"))])
        .with_send_window(1)
        // Reset after 2 bytes of the 4-byte CONNACK go out.
        .with_send_fault(SendFault {
            at_offset: 2,
            kind: SendFaultKind::PeerReset,
        });
    listener.queue_connection(conn);

    let mut broker: BrokerCore<MAX_CLIENTS> = BrokerCore::new();
    let mut slots = new_slots();

    pump_tick(&mut listener, &mut broker, &mut slots, &mut no_render);

    // The slot must be closed — the byte stream desynced mid-CONNACK.
    assert!(
        slots[0].conn.is_none(),
        "slot must be closed after PeerGone"
    );
}

// -- case: orderly peer close ------------------------------------------------------------------

#[test]
fn orderly_peer_close_frees_the_slot() {
    let mut listener = MockBrokerListener::new();
    let conn = MockBrokerConn::new(vec![MockRecvEvent::Bytes(encode_connect_packet("dev01"))]);
    listener.queue_connection(conn);

    let mut broker: BrokerCore<MAX_CLIENTS> = BrokerCore::new();
    let mut slots = new_slots();

    // Tick 1: CONNECT -> CONNACK.
    pump_tick(&mut listener, &mut broker, &mut slots, &mut no_render);
    assert!(slots[0].conn.is_some());

    // Script an orderly close for tick 2.
    slots[0].conn.as_mut().unwrap().recv_script_push_closed();

    pump_tick(&mut listener, &mut broker, &mut slots, &mut no_render);
    assert!(slots[0].conn.is_none(), "orderly close must free the slot");
}

// -- case: WouldBlock-on-recv (no data this tick) does not disturb the slot -------------------

#[test]
fn would_block_leaves_slot_open_and_untouched() {
    let mut listener = MockBrokerListener::new();
    let conn = MockBrokerConn::new(vec![
        MockRecvEvent::Bytes(encode_connect_packet("dev01")),
        MockRecvEvent::WouldBlock,
        MockRecvEvent::WouldBlock,
    ]);
    listener.queue_connection(conn);

    let mut broker: BrokerCore<MAX_CLIENTS> = BrokerCore::new();
    let mut slots = new_slots();

    pump_tick(&mut listener, &mut broker, &mut slots, &mut no_render); // CONNECT
    let after_connect = slots[0].conn.as_ref().unwrap().sent.len();

    pump_tick(&mut listener, &mut broker, &mut slots, &mut no_render); // WouldBlock tick
    pump_tick(&mut listener, &mut broker, &mut slots, &mut no_render); // WouldBlock tick

    assert!(
        slots[0].conn.is_some(),
        "WouldBlock must never close the slot"
    );
    assert_eq!(
        slots[0].conn.as_ref().unwrap().sent.len(),
        after_connect,
        "no bytes should be sent on WouldBlock ticks"
    );
}

// -- case: interleaved recv/send across multiple ticks, byte-exact end to end ----------------

#[test]
fn interleaved_recv_send_is_byte_exact_end_to_end() {
    let mut listener = MockBrokerListener::new();
    let conn = MockBrokerConn::new(vec![
        MockRecvEvent::Bytes(encode_connect_packet("dev01")),
        MockRecvEvent::WouldBlock,
        MockRecvEvent::Bytes(encode_subscribe(7, "tms/dev01/reading")),
    ])
    .with_send_window(8);
    listener.queue_connection(conn);

    let mut broker: BrokerCore<MAX_CLIENTS> = BrokerCore::new();
    let mut slots = new_slots();

    // Tick 1: CONNECT -> CONNACK.
    pump_tick(&mut listener, &mut broker, &mut slots, &mut no_render);
    let ack = parse_connack(&slots[0].conn.as_ref().unwrap().sent).unwrap();
    assert_eq!(ack.return_code, 0);
    let after_connack = slots[0].conn.as_ref().unwrap().sent.len();

    // Tick 2: WouldBlock — nothing new.
    pump_tick(&mut listener, &mut broker, &mut slots, &mut no_render);
    assert_eq!(slots[0].conn.as_ref().unwrap().sent.len(), after_connack);

    // Tick 3: SUBSCRIBE -> SUBACK.
    pump_tick(&mut listener, &mut broker, &mut slots, &mut no_render);
    let after_suback = slots[0].conn.as_ref().unwrap().sent.len();
    assert!(after_suback > after_connack, "SUBACK must have been sent");

    // Publish + drain on a later tick — proves publish fanout still reaches the client after the
    // interleaved WouldBlock tick.
    broker.publish_topic("tms/dev01/reading", true);
    let mut render = |topic: &str, out: &mut [u8]| -> Option<usize> {
        if topic == "tms/dev01/reading" {
            let payload = b"{\"v\":42}";
            out.get_mut(..payload.len())?.copy_from_slice(payload);
            Some(payload.len())
        } else {
            None
        }
    };
    pump_tick(&mut listener, &mut broker, &mut slots, &mut render);
    let sent = &slots[0].conn.as_ref().unwrap().sent;
    assert!(
        sent.ends_with(b"{\"v\":42}"),
        "rendered payload must be the exact tail of the stream"
    );
}
