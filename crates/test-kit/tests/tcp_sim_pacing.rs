//! `--pace` wire-rate pacing acceptance test for the `seplos-tcp-sim` binary.
//!
//! Spawns the actual compiled `seplos-tcp-sim` binary (never re-implements the pacing logic
//! itself) and observes reply bytes arriving on a real TCP socket, timed with
//! `std::time::Instant`, to confirm: paced replies trickle out over roughly the expected wire
//! time instead of bursting, and unpaced (default) replies stay effectively instant.
//!
//! New file (does not touch `tcp_sim.rs`, which exercises `SeplosSim::respond` directly).

#![cfg(feature = "tcp-sim")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Verbatim DIP0 `0x42` telemetry request (`lib-parse/tests/seplos_vectors.rs`) — the same vector
/// `tcp_sim.rs` uses, never re-derived here.
const REQUEST_0X42_DIP0: &[u8] = b"~20004642F0010FD67\r";

/// Path to the compiled `seplos-tcp-sim` binary, resolved by cargo at test-build time.
const SIM_BIN: &str = env!("CARGO_BIN_EXE_seplos-tcp-sim");

/// Kills the spawned sim process (and reaps it) on drop, so a failing assertion never leaks a
/// background process across test runs.
struct SimGuard(Child);

impl Drop for SimGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Spawn `seplos-tcp-sim --listen <ephemeral-port> <extra_args>` and return a guard (kills the
/// child on drop) plus a connected client `TcpStream`.
fn spawn_sim(extra_args: &[&str]) -> (SimGuard, TcpStream) {
    // Reserve an ephemeral port by binding then releasing it; the sim binds it next. This has a
    // small accepted TOCTOU window (fine for a single-threaded local test, not a shipping path).
    let probe = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = probe.local_addr().expect("local_addr").port();
    drop(probe);

    let mut cmd = Command::new(SIM_BIN);
    cmd.arg("--listen")
        .arg(port.to_string())
        .args(extra_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = cmd.spawn().expect("spawn seplos-tcp-sim");
    let guard = SimGuard(child);

    // Poll-connect: the sim needs a moment to bind+accept after spawn.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(stream) => {
                stream.set_nodelay(true).expect("set_nodelay on client");
                return (guard, stream);
            }
            Err(_) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => panic!("could not connect to seplos-tcp-sim on port {port}: {e}"),
        }
    }
}

/// Read one full reply frame (terminated by the Seplos EOI byte `\r`), recording the wall-clock
/// offset (from `start`) each byte arrived at, so the caller can assert on trickle-vs-burst
/// delivery.
fn read_timed_reply(stream: &mut TcpStream, start: Instant) -> (Vec<u8>, Vec<Duration>) {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set_read_timeout");
    let mut bytes = Vec::new();
    let mut timestamps = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        stream.read_exact(&mut byte).expect("read reply byte");
        timestamps.push(start.elapsed());
        bytes.push(byte[0]);
        if byte[0] == 0x0D {
            break;
        }
    }
    (bytes, timestamps)
}

#[test]
fn paced_reply_trickles_at_wire_rate() {
    let (_guard, mut stream) = spawn_sim(&["--pace", "19200"]);

    let start = Instant::now();
    stream.write_all(REQUEST_0X42_DIP0).expect("write request");
    stream.flush().expect("flush request");

    let (bytes, timestamps) = read_timed_reply(&mut stream, start);
    let n = bytes.len();
    assert!(
        n > 32,
        "expected a full telemetry reply, got {n} bytes: {bytes:?}"
    );

    let first = timestamps[0];
    let last = *timestamps.last().expect("at least one byte arrived");

    // Ground truth: 10 bits/byte at 19200 baud (8N1: 1 start + 8 data + 1 stop) => n * 10 / 19200
    // seconds of pure wire time, plus the default ~10ms turnaround before the first byte. A wide
    // +-2x/-0.5x band (plus a flat 200ms pad) absorbs CI scheduling jitter without losing the
    // "actually paced, not bursted" signal this test exists to catch.
    let expected_wire = Duration::from_secs_f64(n as f64 * 10.0 / 19_200.0);
    let min_total = expected_wire.mul_f64(0.5);
    let max_total = expected_wire.mul_f64(2.0) + Duration::from_millis(200);

    assert!(
        last >= min_total,
        "paced reply finished too fast: {last:?} < {min_total:?} ({n} bytes)"
    );
    assert!(
        last <= max_total,
        "paced reply finished too slow: {last:?} > {max_total:?} ({n} bytes)"
    );

    // Bytes must TRICKLE across the frame, not arrive as one burst.
    let spread = last.saturating_sub(first);
    let min_spread = expected_wire.mul_f64(0.25);
    assert!(
        spread >= min_spread,
        "reply arrived in a burst (first={first:?}, last={last:?}, spread={spread:?} < \
         {min_spread:?}); expected pacing across the frame"
    );
}

#[test]
fn unpaced_reply_remains_instant() {
    let (_guard, mut stream) = spawn_sim(&[]);

    let start = Instant::now();
    stream.write_all(REQUEST_0X42_DIP0).expect("write request");
    stream.flush().expect("flush request");

    let (bytes, timestamps) = read_timed_reply(&mut stream, start);
    assert!(!bytes.is_empty(), "expected a non-empty reply");
    let last = *timestamps.last().expect("at least one byte arrived");
    // The task spec's illustrative bound is <5ms for the write itself; this generously widens to
    // <50ms to absorb process-spawn/scheduling jitter around the full TCP round trip in CI, while
    // still being an order of magnitude below the >=75ms a paced 19200-baud reply takes.
    assert!(
        last < Duration::from_millis(50),
        "unpaced reply took too long: {last:?} (expected a near-instant burst)"
    );
}
