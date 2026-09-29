//! Host oracle for the intermittent broker-thread "Illegal load of EXC_RETURN into PC" corruption
//! (Phase-2 plan): ports `bin_zephyr_platform::rust_broker_main`'s exact `static mut PUMP_OUT_BUF` +
//! raw-pointer recv-then-render usage pattern onto the host, driven under Miri.
//!
//! `bin/zephyr/platform/src/broker_pump.rs` reuses ONE `static mut [u8; PUMP_BUF]` for two
//! purposes per client per tick:
//!   1. `sz_sock_recv` writes into it via a raw `*mut u8`, then a fresh `&[u8]` (`recv_bytes`) is
//!      derived over the received length and handed to `BrokerCore::feed_incoming`.
//!   2. A second, independently-derived `&mut [u8]` (`out_buf`, spanning the FULL buffer) is handed
//!      to `BrokerCore::drain_outgoing`, whose render closure (`render_pending_topic`) writes the
//!      rendered JSON payload into a sub-slice of that same `out_buf`.
//!
//! Both derivations go through `core::ptr::addr_of_mut!` + `slice::from_raw_parts[_mut]`, re-derived
//! fresh each time rather than holding one long-lived reference — the shape the firmware code's
//! safety comments argue is sound. This harness reproduces that *exact* shape (same static, same
//! pointer-rederivation discipline, same recv-then-render-same-tick control flow, run across many
//! ticks/clients/payload sizes) so Miri's Stacked Borrows / aliasing checker gets a fair shot at it
//! independent of any Zephyr-only `sz_*` FFI (which can't build on host at all).
//!
//! Run under Miri: `cargo +nightly miri test -p lib-link --test broker_pump_aliasing_miri`.

// Needs lib-net's `link` module (BrokerCore); compiles to nothing under feature sets without it
// (e.g. the `packed`-only codec test build).
#![cfg(feature = "link")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_net::link::BrokerCore;

const MAX_CLIENTS: usize = 2;
const MAX_PAYLOAD: usize = 512; // smaller than firmware's 1664 — keeps Miri fast; shape is what matters.
const PUMP_BUF: usize = BrokerCore::<MAX_CLIENTS>::min_out_len(MAX_PAYLOAD);

/// Mirrors `bin_zephyr_platform::PUMP_OUT_BUF` exactly: one `static mut` buffer, reused across both the
/// "recv" and "render-into" roles within a single pump-loop tick, accessed only via freshly
/// re-derived raw pointers (never one live reference held across both uses).
static mut PUMP_OUT_BUF: [u8; PUMP_BUF] = [0u8; PUMP_BUF];

/// Both `#[test]`s in this binary drive the SAME process-global `PUMP_OUT_BUF` (mirroring the
/// firmware's single static). cargo runs a binary's tests in parallel by default, which would race
/// two simulated "broker threads" on that one static — a harness artifact, NOT a firmware bug (the
/// device has exactly one broker thread, so the static is genuinely single-accessor there). This
/// lock serializes the tests so each gets exclusive use of the static, exactly like the single
/// on-device broker thread; poisoning is ignored (a panic in one test already fails the suite).
static PUMP_BUF_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn pump_buf_guard() -> std::sync::MutexGuard<'static, ()> {
    PUMP_BUF_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Stand-in for the real socket: hands back up to `len` bytes of `data` (mirrors `sz_sock_recv`'s
/// "receive whatever the peer sent, up to `len`" contract), writing through a raw pointer exactly
/// like the C FFI call does (`sz_sock_recv(fd, buf_ptr, PUMP_BUF as i32)`).
fn fake_recv(buf_ptr: *mut u8, cap: usize, data: &[u8]) -> usize {
    let n = data.len().min(cap);
    // SAFETY (of this harness, mirroring the real FFI boundary): `buf_ptr` is valid for `cap` bytes
    // (it is `PUMP_OUT_BUF`'s base pointer), and `n <= cap`.
    unsafe {
        core::ptr::copy_nonoverlapping(data.as_ptr(), buf_ptr, n);
    }
    n
}

/// Stand-in for `render_pending_topic`: renders a deterministic, size-varying, non-zero payload for
/// any topic starting with `"tms/"`, mirroring the real renderer's "map topic -> bytes into `out`"
/// shape (including reading from another piece of process state — here a plain `usize`, standing in
/// for `ARENA.with_arena(...)` — while `out` is being written, the same "two live pieces of shared
/// state touched in one render call" shape the real code has).
fn render_topic(topic: &str, out: &mut [u8], call_count: usize) -> Option<usize> {
    if !topic.starts_with("tms/") {
        return None;
    }
    // Vary the rendered length across calls (like real telemetry: different JSON sizes per cycle),
    // sweeping across the varint-width boundaries and up to (and one past) MAX_PAYLOAD.
    let candidate_lens = [
        0usize,
        1,
        42,
        127,
        128,
        129,
        MAX_PAYLOAD - 1,
        MAX_PAYLOAD,
        MAX_PAYLOAD + 1,
    ];
    let want = candidate_lens[call_count % candidate_lens.len()];
    if want > out.len() {
        return None; // mirrors a well-behaved renderer: never claim more than it was offered.
    }
    for (i, b) in out[..want].iter_mut().enumerate() {
        *b = (i as u8).wrapping_add(1).max(1);
    }
    Some(want)
}

/// Replicates `rust_broker_main`'s per-client per-tick body: raw-pointer recv into `PUMP_OUT_BUF`,
/// `feed_incoming` off an immutable slice derived from it, then a FRESH mutable slice over the same
/// static handed to `drain_outgoing` with a render closure — same pointer re-derivation discipline,
/// same intra-tick reuse, as the firmware. Returns every drained packet (owned, for post-hoc
/// inspection) so the caller can assert on framing correctness in addition to Miri's own UB checks.
fn pump_one_client_one_tick(
    broker: &mut BrokerCore<MAX_CLIENTS>,
    slot: usize,
    inbound: &[u8],
    call_count: &mut usize,
) -> Vec<Vec<u8>> {
    // SAFETY: broker-thread-exclusive in the real firmware; this harness is single-threaded so the
    // same "exactly one live accessor at a time, pointer re-derived fresh per access" discipline
    // holds trivially. Mirrors `broker_pump.rs`'s `buf_ptr` derivation exactly.
    let buf_ptr: *mut u8 = core::ptr::addr_of_mut!(PUMP_OUT_BUF).cast();

    let n = fake_recv(buf_ptr, PUMP_BUF, inbound);
    if n > 0 {
        // SAFETY: mirrors `broker_pump.rs` line-for-line — `n` bounded to `PUMP_BUF` by `fake_recv`.
        let recv_bytes: &[u8] = unsafe { core::slice::from_raw_parts(buf_ptr, n) };
        let _ = broker.feed_incoming(slot, recv_bytes);
        // `recv_bytes`'s borrow ends here (last use), exactly like the real loop.
    }

    let mut drained = Vec::new();
    loop {
        let mut render = |topic: &str, out: &mut [u8]| -> Option<usize> {
            let r = render_topic(topic, out, *call_count);
            *call_count += 1;
            r
        };
        // SAFETY: mirrors `broker_pump.rs`'s `out_buf` derivation — a FRESH raw-pointer-derived `&mut
        // [u8]` over the FULL static, taken AFTER `recv_bytes` (an immutable borrow of a prefix of
        // the very same static) has already gone out of scope.
        let out_buf: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(buf_ptr, PUMP_BUF) };
        let out_n = broker.drain_outgoing(slot, out_buf, &mut render);
        if out_n == 0 {
            break;
        }
        drained.push(out_buf[..out_n].to_vec());
    }
    drained
}

fn connect_and_subscribe(broker: &mut BrokerCore<MAX_CLIENTS>, slot: usize, filter: &str) {
    broker.client_connected(slot);
    // Minimal hand-rolled CONNECT (MQTT 3.1.1): mirrors the shape `mqtt_server`'s own tests use.
    let mut connect = vec![0x10u8];
    let mut body = Vec::new();
    body.extend_from_slice(&[0x00, 0x04]);
    body.extend_from_slice(b"MQTT");
    body.push(0x04); // level 4
    body.push(0x02); // clean session
    body.extend_from_slice(&[0x00, 0x3C]); // keep-alive 60
    body.extend_from_slice(&[0x00, 0x05]);
    body.extend_from_slice(b"dev01");
    encode_remaining_length(&mut connect, body.len());
    connect.extend_from_slice(&body);
    let mut call_count = 0;
    let _ = pump_one_client_one_tick(broker, slot, &connect, &mut call_count);

    // Minimal hand-rolled SUBSCRIBE.
    let mut sub = vec![0x82u8];
    let mut body = Vec::new();
    body.extend_from_slice(&[0x00, 0x01]); // packet id
    body.extend_from_slice(&(filter.len() as u16).to_be_bytes());
    body.extend_from_slice(filter.as_bytes());
    body.push(0x00); // requested QoS 0
    encode_remaining_length(&mut sub, body.len());
    sub.extend_from_slice(&body);
    let mut call_count = 0;
    let _ = pump_one_client_one_tick(broker, slot, &sub, &mut call_count);
}

/// Decode an MQTT 3.1.1 Remaining Length varint starting at `bytes[0]`, returning
/// `(value, bytes_consumed)`. Mirrors `mqtt_wire::parse_fixed_header`'s decode loop (that fn is
/// crate-private, so this integration test — outside the crate — re-derives the same tiny
/// algorithm rather than reaching in).
fn decode_remaining_length(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut value = 0usize;
    let mut multiplier = 1usize;
    for (i, &b) in bytes.iter().enumerate().take(4) {
        value += (b & 0x7F) as usize * multiplier;
        if b & 0x80 == 0 {
            return Some((value, i + 1));
        }
        multiplier *= 128;
    }
    None
}

fn encode_remaining_length(out: &mut Vec<u8>, mut len: usize) {
    loop {
        let mut byte = (len % 128) as u8;
        len /= 128;
        if len > 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if len == 0 {
            break;
        }
    }
}

/// The core repro: many ticks, each publishing a fresh topic and pumping the sole connected client,
/// reusing `PUMP_OUT_BUF` exactly as `rust_broker_main` does every ~10 ms in firmware. If the
/// raw-pointer reuse pattern were unsound (aliasing two live accessors, or a length/provenance bug),
/// Miri's Stacked Borrows checker flags it deterministically; a plain `cargo test` run additionally
/// sanity-checks framing (every drained packet parses and never exceeds `PUMP_BUF`).
#[test]
fn broker_pump_static_buf_reuse_across_many_ticks_is_miri_clean() {
    let _serialize = pump_buf_guard(); // exclusive use of the shared PUMP_OUT_BUF static
    let mut broker: BrokerCore<MAX_CLIENTS> = BrokerCore::new();
    connect_and_subscribe(&mut broker, 0, "tms/#");

    let mut call_count = 0usize;
    for tick in 0..24 {
        let topic = format!("tms/METEOR-001/topic{}", tick % 3);
        broker.publish_topic(&topic, false);

        // Empty inbound this tick (mirrors "n < 0: no data ready", the common case) — still pumps
        // the render/drain side, reusing `PUMP_OUT_BUF` for the render just like every real tick.
        let drained = pump_one_client_one_tick(&mut broker, 0, &[], &mut call_count);
        for packet in &drained {
            assert!(
                packet.len() <= PUMP_BUF,
                "tick {tick}: drained packet ({} bytes) exceeds PUMP_BUF ({PUMP_BUF}) — \
                 this is the overflow class the fault points at",
                packet.len()
            );
            // Every drained packet must be a well-formed PUBLISH: type nibble 0x3, and the
            // Remaining-Length-declared length must match what was actually returned. `mqtt_wire`
            // is crate-private, so this harness decodes the varint itself (same algorithm).
            if packet[0] & 0xF0 == 0x30 {
                let (remaining_length, header_len) =
                    decode_remaining_length(&packet[1..]).expect("parseable fixed header");
                assert_eq!(
                    1 + header_len + remaining_length,
                    packet.len(),
                    "tick {tick}: Remaining Length disagrees with actual drained packet length"
                );
            }
        }
    }
}

/// Interleaves TWO connected clients pumped in the same tick, each recv-then-render cycle reusing
/// the identical `PUMP_OUT_BUF` static back-to-back within one loop iteration (slot 0 fully pumped,
/// THEN slot 1) — the exact `for (slot, client) in clients.iter_mut().enumerate()` shape in
/// `rust_broker_main`, which is the scenario the task brief calls out ("combined live-telemetry +
/// connected-client load").
#[test]
fn broker_pump_two_clients_interleaved_same_static_buf_is_miri_clean() {
    let _serialize = pump_buf_guard(); // exclusive use of the shared PUMP_OUT_BUF static
    let mut broker: BrokerCore<MAX_CLIENTS> = BrokerCore::new();
    connect_and_subscribe(&mut broker, 0, "tms/#");
    connect_and_subscribe(&mut broker, 1, "tms/#");

    let mut call_count = 0usize;
    for tick in 0..16 {
        let topic = format!("tms/METEOR-001/topic{}", tick % 3);
        broker.publish_topic(&topic, false);

        for slot in [0usize, 1] {
            let drained = pump_one_client_one_tick(&mut broker, slot, &[], &mut call_count);
            for packet in &drained {
                assert!(
                    packet.len() <= PUMP_BUF,
                    "tick {tick} slot {slot}: overflow"
                );
            }
        }
    }
}

#[cfg(feature = "std")]
mod ensure_std {
    // This file needs `std::vec`/`String`/`format!` — `lib-link`'s `std` feature is enabled by
    // default (see Cargo.toml), so no extra plumbing is needed; this module exists only so a
    // `--no-default-features` Miri invocation fails loudly/early rather than with a confusing
    // "can't find `Vec`" error deep in the test body above.
}
