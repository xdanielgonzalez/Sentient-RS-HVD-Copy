//! T2 — host read timeout, proven against a real pty (the shipping tokio-serial transport).
//!
//! Before this slice, `HostSerial::read_until` looped on `read_exact` with no deadline: a silent
//! or half-responding RS-485 bus hung the poll loop forever. These tests open a real pty, drive
//! the *actual* `HostSerial` transport over the slave path, and prove:
//!   * a silent master → `IoError::Timeout` within the deadline (never hangs),
//!   * a half-frame then silence → `IoError::Timeout` (partial reads don't rescue it),
//!   * a complete frame → `Ok`, well under the deadline.
//!
//! Integration tests are their own crate, so the crate-level `forbid(unsafe_code)` does not apply
//! here; `ptsname` is the one unsafe call and it is confined to test bring-up.
#![cfg(all(unix, feature = "host"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::time::{Duration, Instant};

use nix::fcntl::OFlag;
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt, PtyMaster};

use lib_io::host::{HostSerial, OpenError, SerialKind};
use lib_io::transport::{IoError, SerialParams, SerialTransport};

/// Open a pty pair; return the master handle and the slave device path (`/dev/pts/N`).
fn open_pty() -> (PtyMaster, String) {
    let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).expect("posix_openpt");
    grantpt(&master).expect("grantpt");
    unlockpt(&master).expect("unlockpt");
    // SAFETY (test-only): single-threaded bring-up; the returned path is used immediately.
    let slave = unsafe { ptsname(&master) }.expect("ptsname");
    (master, slave)
}

/// Open the real host transport against the pty slave with a short read deadline.
fn open_transport(slave: &str, read_timeout_ms: u32) -> HostSerial {
    HostSerial::open(
        slave,
        SerialKind::Rs232,
        SerialParams::default(),
        read_timeout_ms,
    )
    .expect("open pty slave as serial")
}

#[serial_test::file_serial(seplos_e2e)]
#[tokio::test]
async fn silent_bus_times_out_without_hanging() {
    let (_master, slave) = open_pty();
    let mut serial = open_transport(&slave, 200);
    let mut buf = [0u8; 256];

    let start = Instant::now();
    let res = serial.read_until(b'\r', &mut buf).await;
    let elapsed = start.elapsed();

    assert_eq!(res, Err(IoError::Timeout), "a silent master must time out");
    assert!(
        elapsed < Duration::from_millis(1500),
        "must return near the 200ms deadline, took {elapsed:?}"
    );
    // Keep _master alive until here so the slave path stays valid for the whole read.
}

#[serial_test::file_serial(seplos_e2e)]
#[tokio::test]
async fn half_frame_then_silence_times_out() {
    let (master, slave) = open_pty();
    let mut serial = open_transport(&slave, 200);

    // Write a partial frame with no trailing delimiter, then go silent.
    {
        let mut f = unsafe { std::fs::File::from_raw_fd_unowned(master.as_raw_fd()) };
        let _ = f.write_all(b"~2000460");
        std::mem::forget(f); // don't close the master fd; PtyMaster owns it
    }

    let mut buf = [0u8; 256];
    let res = serial.read_until(b'\r', &mut buf).await;
    assert_eq!(
        res,
        Err(IoError::Timeout),
        "an unterminated partial frame must still time out"
    );
    drop(master);
}

#[serial_test::file_serial(seplos_e2e)]
#[tokio::test]
async fn complete_frame_reads_ok_before_deadline() {
    let (master, slave) = open_pty();
    let mut serial = open_transport(&slave, 1000);

    let frame = b"~20004600F0010FD5C\r";
    {
        let mut f = unsafe { std::fs::File::from_raw_fd_unowned(master.as_raw_fd()) };
        f.write_all(frame).expect("write frame to master");
        f.flush().ok();
        std::mem::forget(f);
    }

    let mut buf = [0u8; 256];
    let start = Instant::now();
    let n = serial
        .read_until(b'\r', &mut buf)
        .await
        .expect("complete frame reads Ok");
    assert!(
        start.elapsed() < Duration::from_millis(900),
        "a full frame must return well under the deadline"
    );
    assert_eq!(
        &buf[..n],
        frame,
        "delivered bytes match what the master wrote"
    );
    drop(master);
}

#[test]
fn open_error_carries_os_detail() {
    // T5: a missing device must surface a diagnosable error, not a detail-free IoError::Device.
    let res = HostSerial::open(
        "/dev/tty-sentient-does-not-exist",
        SerialKind::Rs232,
        SerialParams::default(),
        1000,
    );
    let err = match res {
        Ok(_) => panic!("opening a nonexistent device must fail"),
        Err(e) => e,
    };
    match err {
        OpenError::Io(_) => {
            let msg = format!("{err}");
            assert!(
                !msg.is_empty(),
                "OpenError Display must carry the OS detail"
            );
        }
        OpenError::InvalidParams(p) => panic!("unexpected InvalidParams: {p}"),
        // #245: `OpenError` grew a third variant (`Direction`, for a `KernelRs485`/`GpioDe`
        // direction mechanism that fails to initialize) — irrelevant here (`SerialKind::Rs232`
        // never takes the RS-485 direction-control path), added only to keep this pre-existing
        // exhaustive match compiling.
        OpenError::Direction(d) => panic!("unexpected Direction: {d}"),
    }
}

/// Thin extension: borrow a raw fd as a `File` without taking ownership (so `PtyMaster` keeps it).
trait FromRawFdUnowned {
    /// # Safety
    /// The caller must `std::mem::forget` the returned `File` so the fd is not double-closed.
    unsafe fn from_raw_fd_unowned(fd: std::os::unix::io::RawFd) -> Self;
}
impl FromRawFdUnowned for std::fs::File {
    unsafe fn from_raw_fd_unowned(fd: std::os::unix::io::RawFd) -> Self {
        use std::os::unix::io::FromRawFd;
        std::fs::File::from_raw_fd(fd)
    }
}
