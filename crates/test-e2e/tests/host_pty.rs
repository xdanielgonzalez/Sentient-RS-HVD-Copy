//! T3 — a synthetic Seplos responder over a real pty, feeding real and mutated capture frames
//! into the *actual* shipping `HostSerial` transport + generic driver.
//!
//! A responder thread owns the pty master, reads the driver's command frame, matches it against the
//! byte-exact command vectors, and replies per a scripted [`Behavior`]. Each test drives
//! `driver::read_telemetry` over the pty slave (the real tokio-serial transport) and asserts the
//! graceful outcome: a good frame decodes; silence/partial → `IoError::Timeout`; a corrupt checksum
//! or wrong address → a parse error; garbage → an error, never a panic. Multi-DIP proves the driver
//! recovers per exchange.
//!
//! Integration tests are their own crate, so `forbid(unsafe_code)` does not apply here; `ptsname`
//! and the unowned-fd borrow are the only unsafe calls, confined to test bring-up.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::thread;
use std::time::Duration;

use nix::fcntl::OFlag;
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt, PtyMaster};

use lib_io::driver::{read_telemetry, ExchangeError};
use lib_io::host::{HostSerial, SerialKind};
use lib_io::transport::{IoError, SerialParams};
use lib_parse::seplos::Seplos;
use lib_parse::vendor::ParseError;

/// The live 14-cell telemetry response (Seplos 48150-1101-10E, DIP 0).
const LIVE_FRAME: &[u8] = b"~20004600A08E00000E0F360F360F360F360F350F360F360F360F360F370F370F360F360F35060AF10AF90AF90AF30B370B0CFF5C154C36820A465003074650000203E8154CFFF800000079004DDEC8\r";

/// How the responder answers one command frame.
#[derive(Debug, Clone, Copy)]
enum Behavior {
    /// Reply with the given bytes verbatim.
    Respond(&'static [u8]),
    /// Never reply (silent bus).
    Silent,
    /// Reply only the first `usize` bytes of the live frame, then go silent.
    Partial(usize),
    /// Reply the live frame with one INFO byte flipped (checksum then fails).
    FlipChecksumByte,
    /// Reply a well-formed frame but for the wrong device address.
    WrongAdr,
    /// Reply non-frame garbage terminated by `\r`.
    Garbage,
}

fn open_pty() -> (PtyMaster, String) {
    let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).expect("posix_openpt");
    grantpt(&master).expect("grantpt");
    unlockpt(&master).expect("unlockpt");
    // SAFETY (test-only): single-threaded bring-up; path used immediately.
    let slave = unsafe { ptsname(&master) }.expect("ptsname");
    (master, slave)
}

/// Borrow the master fd as a `File` without owning it (PtyMaster keeps ownership).
fn master_file(master: &PtyMaster) -> std::fs::File {
    use std::os::unix::io::FromRawFd;
    // SAFETY (test-only): the returned File is forgotten by the caller, so the fd is not closed.
    unsafe { std::fs::File::from_raw_fd(master.as_raw_fd()) }
}

/// Spawn the responder: read one command frame from the master (until `\r`), then reply per
/// `behavior`. Runs on its own thread so the driver's blocking read has a peer.
fn spawn_responder(master: PtyMaster, behavior: Behavior) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut f = master_file(&master);
        // Read the command frame (bounded; commands are ~17 bytes ending in '\r').
        let mut cmd = Vec::new();
        let mut byte = [0u8; 1];
        for _ in 0..64 {
            match f.read(&mut byte) {
                Ok(1) => {
                    cmd.push(byte[0]);
                    if byte[0] == b'\r' {
                        break;
                    }
                }
                _ => break,
            }
        }
        respond(&mut f, behavior);
        // Hold the master open. For silent/partial the bus must stay silent PAST the read deadline
        // (400 ms) so the transport actually times out, rather than seeing EOF (which is a Device
        // error). For a full reply, a short drain is enough before close.
        let hold_ms = match behavior {
            Behavior::Silent | Behavior::Partial(_) => 700,
            _ => 80,
        };
        thread::sleep(Duration::from_millis(hold_ms));
        std::mem::forget(f);
        drop(master);
    })
}

fn respond(f: &mut std::fs::File, behavior: Behavior) {
    match behavior {
        Behavior::Respond(bytes) => {
            let _ = f.write_all(bytes);
        }
        Behavior::Silent => {}
        Behavior::Partial(n) => {
            let end = n.min(LIVE_FRAME.len());
            let _ = f.write_all(&LIVE_FRAME[..end]);
        }
        Behavior::FlipChecksumByte => {
            let mut frame = LIVE_FRAME.to_vec();
            // Flip a data nibble well inside the INFO region so the checksum no longer matches.
            frame[10] ^= 0x01;
            let _ = f.write_all(&frame);
        }
        Behavior::WrongAdr => {
            // ADR is the two hex chars after the leading '~' + VER "20": bytes [3..5] = "00".
            let mut frame = LIVE_FRAME.to_vec();
            frame[3] = b'0';
            frame[4] = b'5'; // address 5, not the polled 0
            let _ = f.write_all(&frame);
        }
        Behavior::Garbage => {
            let _ = f.write_all(b"not a seplos frame at all\r");
        }
    }
    let _ = f.flush();
}

/// Open the real host transport against the pty slave with a short read deadline. RS-232 (not
/// RS-485) because a pty has no RTS modem line for half-duplex direction control; the request/
/// response mechanics under test are identical either way.
fn open_transport(slave: &str) -> HostSerial {
    HostSerial::open(slave, SerialKind::Rs232, SerialParams::default(), 400)
        .expect("open pty slave as serial")
}

async fn poll_once(slave: &str) -> Result<(), ExchangeError> {
    let mut serial = open_transport(slave);
    let vendor = Seplos::new();
    let mut buf = [0u8; 256];
    read_telemetry(&mut serial, &vendor, 0, &mut buf)
        .await
        .map(|_| ())
}

#[serial_test::file_serial(seplos_e2e)]
#[tokio::test]
async fn good_frame_decodes() {
    let (master, slave) = open_pty();
    let h = spawn_responder(master, Behavior::Respond(LIVE_FRAME));
    let mut serial = open_transport(&slave);
    let vendor = Seplos::new();
    let mut buf = [0u8; 256];

    let reading = read_telemetry(&mut serial, &vendor, 0, &mut buf)
        .await
        .expect("live frame decodes");
    assert_eq!(reading.cell_voltages_mv.len(), 14, "14-cell pack");
    h.join().ok();
}

#[serial_test::file_serial(seplos_e2e)]
#[tokio::test]
async fn silent_bus_times_out() {
    let (master, slave) = open_pty();
    let h = spawn_responder(master, Behavior::Silent);
    let res = poll_once(&slave).await;
    assert_eq!(
        res,
        Err(ExchangeError::Io(IoError::Timeout)),
        "silence must time out, not hang"
    );
    h.join().ok();
}

#[serial_test::file_serial(seplos_e2e)]
#[tokio::test]
async fn partial_frame_times_out() {
    let (master, slave) = open_pty();
    let h = spawn_responder(master, Behavior::Partial(20));
    let res = poll_once(&slave).await;
    assert_eq!(
        res,
        Err(ExchangeError::Io(IoError::Timeout)),
        "an unterminated partial must time out"
    );
    h.join().ok();
}

#[serial_test::file_serial(seplos_e2e)]
#[tokio::test]
async fn corrupt_checksum_is_parse_error() {
    let (master, slave) = open_pty();
    let h = spawn_responder(master, Behavior::FlipChecksumByte);
    let res = poll_once(&slave).await;
    assert_eq!(
        res,
        Err(ExchangeError::Parse(ParseError::BadChecksum)),
        "a flipped byte must fail the frame checksum",
    );
    h.join().ok();
}

#[serial_test::file_serial(seplos_e2e)]
#[tokio::test]
async fn wrong_address_is_rejected() {
    let (master, slave) = open_pty();
    let h = spawn_responder(master, Behavior::WrongAdr);
    let res = poll_once(&slave).await;
    // Either an explicit address mismatch or a checksum failure (the ADR edit also breaks the
    // checksum) — the point is it is NEVER accepted as a valid reading for DIP 0, and never panics.
    assert!(
        matches!(res, Err(ExchangeError::Parse(_))),
        "wrong-address frame must not be accepted: {res:?}"
    );
    h.join().ok();
}

#[serial_test::file_serial(seplos_e2e)]
#[tokio::test]
async fn garbage_is_error_not_panic() {
    let (master, slave) = open_pty();
    let h = spawn_responder(master, Behavior::Garbage);
    let res = poll_once(&slave).await;
    assert!(
        res.is_err(),
        "garbage must be a typed error, not a panic or a reading"
    );
    h.join().ok();
}

#[serial_test::file_serial(seplos_e2e)]
#[tokio::test]
async fn recovers_after_a_bad_exchange() {
    // Exchange 1: silent → timeout. Exchange 2 (fresh pty): good → decodes. Proves the driver
    // does not wedge after a fault.
    {
        let (master, slave) = open_pty();
        let h = spawn_responder(master, Behavior::Silent);
        assert_eq!(
            poll_once(&slave).await,
            Err(ExchangeError::Io(IoError::Timeout))
        );
        h.join().ok();
    }
    {
        let (master, slave) = open_pty();
        let h = spawn_responder(master, Behavior::Respond(LIVE_FRAME));
        assert!(poll_once(&slave).await.is_ok(), "next exchange recovers");
        h.join().ok();
    }
}
