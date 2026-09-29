//! #245 — RS-485 direction-control seam, proven against a real pty (the shipping tokio-serial
//! transport), mirroring `host_timeout.rs`'s real-pty pattern.
//!
//! **Hard-won constraint, verified with a standalone raw-ioctl probe against this sandbox's pty
//! before writing these tests (see the "Bounded Debugging" rule in CLAUDE.md — this is the
//! datum, not a guess):** a Linux pty does NOT implement the modem-control ioctls
//! (`TIOCMGET`/`TIOCMSET`/`TIOCMBIS`) that `tokio_serial::SerialStream::write_request_to_send`
//! needs — every call returns `ENOTTY` ("Inappropriate ioctl for device"). This is NOT new or
//! specific to this workstream: it means `DirectionMode::RtsSoftware`'s `Direction::Transmit`
//! path (`self.stream.write_request_to_send(true)`, unchanged pre-existing logic) has NEVER been
//! exercisable end-to-end over a pty — which is exactly why no test anywhere in this workspace
//! (`host_timeout.rs`, `host_pty.rs`, `seplos_at_rate_e2e.rs`, …) opens a `SerialKind::Rs485` port
//! and calls `set_direction`; they all use `SerialKind::Rs232`, whose `set_direction` never
//! touches RTS at all.
//!
//! What a pty CAN still prove here, worked around that constraint:
//!   * The `Direction::Receive` transition's drain step runs BEFORE the (always-`ENOTTY`-on-a-pty)
//!     RTS deassert attempt: `flush` + `tcdrain(2)` both succeed on a pty (independently verified
//!     with the same raw-ioctl-probe technique — `tcdrain` is a plain termios/line-discipline
//!     operation, unlike the modem-control ioctls), so calling `set_direction(Receive)` DIRECTLY
//!     (skipping the always-failing `Transmit` step) still runs the real drain + `settle_rx_us`
//!     sleep before failing on the final RTS deassert — real, wall-clock-observable timing for the
//!     TX-drain/settle fix this workstream adds, even though the call's end-to-end `Result` is
//!     `Err` on this virtual device (would be `Ok` on a real UART).
//!   * `DirectionMode::KernelRs485` FAILING open loudly: a pty predictably has no `TIOCSRS485`
//!     support (`ENOTTY`) either — exactly the scenario `HostSerial::open_with_direction` must
//!     reject with a message naming the fallback modes, rather than silently starting the port
//!     half configured.
//!
//! What a pty CANNOT prove (documented here instead of skipped silently):
//!   * `DirectionMode::RtsSoftware` succeeding end-to-end (a real UART's RTS line, unlike a pty's,
//!     is a real hardware signal `tokio-serial` can actually toggle);
//!   * `DirectionMode::KernelRs485` actually WORKING (a pty has no hardware UART to configure);
//!   * `DirectionMode::GpioDe` at all (a pty is not a GPIO chip, and this sandbox/CI container has
//!     no `/dev/gpiochipN` to request against).
//!
//! **Field task**: verify all three `DirectionMode`s end-to-end on the shipping RS-485 HAT before
//! enabling any of them in production config — see `lib_io::host`'s module doc comment.
#![cfg(all(unix, feature = "host"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use nix::fcntl::OFlag;
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt, PtyMaster};

use lib_io::host::{DirectionMode, HostSerial, OpenError, SerialKind};
use lib_io::transport::{Direction, IoError, SerialParams, SerialTransport};

/// Open a pty pair; return the master handle and the slave device path (`/dev/pts/N`). Identical
/// to `host_timeout.rs`'s helper of the same name (each integration test file is its own compiled
/// crate, so there is no shared-code seam to reuse this from without a test-support crate).
fn open_pty() -> (PtyMaster, String) {
    let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).expect("posix_openpt");
    grantpt(&master).expect("grantpt");
    unlockpt(&master).expect("unlockpt");
    // SAFETY (test-only): single-threaded bring-up; the returned path is used immediately.
    let slave = unsafe { ptsname(&master) }.expect("ptsname");
    (master, slave)
}

#[serial_test::file_serial(seplos_e2e)]
#[tokio::test]
async fn rts_software_receive_drains_and_settles_before_the_pty_rejects_rts() {
    let (master, slave) = open_pty();
    let direction = DirectionMode::RtsSoftware {
        drain: true,
        settle_tx_us: 0, // irrelevant here — see the module doc's ENOTTY-on-Transmit note
        settle_rx_us: 50_000, // 50ms — long enough to reliably measure, short enough not to be flaky
    };
    let mut serial = HostSerial::open_with_direction(
        &slave,
        SerialKind::Rs485,
        SerialParams::default(),
        1000,
        direction,
    )
    .expect("open pty slave as RS-485 with RtsSoftware direction");

    // Skip `Direction::Transmit` (its `write_request_to_send` always fails with ENOTTY on a pty —
    // see the module doc): call `Receive` directly. The drain step (`flush` + `tcdrain`) and the
    // `settle_rx_us` sleep both still run for real before the final RTS-deassert attempt fails.
    let before = Instant::now();
    let result = serial.set_direction(Direction::Receive).await;
    let elapsed = before.elapsed();

    assert_eq!(
        result,
        Err(IoError::Direction),
        "the drain/settle steps must still run to completion, but the final RTS deassert fails \
         (ENOTTY) on a pty — same as the pre-existing, unmodified Transmit path"
    );
    assert!(
        elapsed >= Duration::from_millis(45),
        "drain (flush + tcdrain, both real syscalls that succeed on a pty) then settle_rx_us \
         (~50ms) must both complete before the RTS-deassert attempt, took {elapsed:?}"
    );

    drop(master);
}

#[serial_test::file_serial(seplos_e2e)]
#[tokio::test]
async fn rts_software_zero_settle_and_no_drain_fails_fast() {
    // Symmetric sanity check: `drain: false` + a zero settle must NOT impose the drain/settle
    // delay — confirms both are actually threaded through per-instance, not hardcoded.
    let (master, slave) = open_pty();
    let direction = DirectionMode::RtsSoftware {
        drain: false,
        settle_tx_us: 0,
        settle_rx_us: 0,
    };
    let mut serial = HostSerial::open_with_direction(
        &slave,
        SerialKind::Rs485,
        SerialParams::default(),
        1000,
        direction,
    )
    .expect("open pty slave as RS-485 with drain-off, zero-settle RtsSoftware direction");

    let before = Instant::now();
    let result = serial.set_direction(Direction::Receive).await;
    let elapsed = before.elapsed();

    assert_eq!(result, Err(IoError::Direction));
    assert!(
        elapsed < Duration::from_millis(500),
        "drain=false + zero settle must not impose the default ~10ms+ delay, took {elapsed:?}"
    );

    drop(master);
}

#[serial_test::file_serial(seplos_e2e)]
#[tokio::test]
async fn rts_software_transmit_fails_with_io_error_direction_on_a_pty() {
    // Documents (rather than hides) the module doc's central constraint: asserting DE via RTS is
    // simply not exercisable end-to-end on a pty. The important property this still proves: the
    // failure is a clean, typed `IoError::Direction` — never a panic or a hang.
    let (master, slave) = open_pty();
    let mut serial = HostSerial::open_with_direction(
        &slave,
        SerialKind::Rs485,
        SerialParams::default(),
        1000,
        DirectionMode::default(),
    )
    .expect("open pty slave as RS-485 with the default RtsSoftware direction");

    let result = serial.set_direction(Direction::Transmit).await;
    assert_eq!(result, Err(IoError::Direction));

    drop(master);
}

#[serial_test::file_serial(seplos_e2e)]
#[tokio::test]
async fn kernel_rs485_fails_open_loudly_naming_fallback_modes_on_a_pty() {
    // A pty predictably has no `TIOCSRS485` support (it isn't a real UART), which is exactly what
    // makes this deterministic without any real RS-485 hardware: `open_with_direction` must reject
    // it with `OpenError::Direction`, not silently hand back a half-configured port.
    let (_master, slave) = open_pty();
    let res = HostSerial::open_with_direction(
        &slave,
        SerialKind::Rs485,
        SerialParams::default(),
        200,
        DirectionMode::KernelRs485,
    );
    let err = match res {
        Ok(_) => panic!("a pty has no TIOCSRS485 support; KernelRs485 must fail open() loudly"),
        Err(e) => e,
    };
    match err {
        OpenError::Direction(msg) => {
            assert!(
                msg.to_lowercase().contains("rts"),
                "must name the rts fallback mode: {msg}"
            );
            assert!(
                msg.to_lowercase().contains("gpio"),
                "must name the gpio fallback mode: {msg}"
            );
        }
        other => panic!(
            "expected OpenError::Direction (TIOCSRS485 ENOTTY/EINVAL on a pty), got {other:?}"
        ),
    }
}

#[serial_test::file_serial(seplos_e2e)]
#[tokio::test]
async fn kernel_rs485_direction_is_irrelevant_for_rs232() {
    // RS-232 is always full-duplex; a configured RS-485 direction mode must be silently ignored
    // (never fail open()) for an RS-232 port — `set_direction` stays a no-op either way (it never
    // touches RTS, unlike RS-485's `RtsSoftware`, so it isn't subject to the ENOTTY-on-a-pty
    // constraint documented at the top of this file).
    let (master, slave) = open_pty();
    let mut serial = HostSerial::open_with_direction(
        &slave,
        SerialKind::Rs232,
        SerialParams::default(),
        200,
        DirectionMode::KernelRs485,
    )
    .expect("RS-232 open must ignore an RS-485-only direction mode, not fail on it");

    assert_eq!(serial.set_direction(Direction::Transmit).await, Ok(()));
    drop(master);
}
