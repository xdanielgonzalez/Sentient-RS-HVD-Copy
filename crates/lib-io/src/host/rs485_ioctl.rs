//! Raw termios/tty control calls behind `DirectionMode::KernelRs485` and
//! `DirectionMode::RtsSoftware`'s drain step — the ONLY unsafe code in `lib-io` (see this
//! crate's `forbid`->`deny` relaxation in `lib.rs`, and CLAUDE.md's *Abstraction & Platform Seams*
//! "keep the FFI/C shim DUMB" rule, which this module follows: each function below is a single,
//! one-shot syscall passthrough — no loop, no retry, no policy. Deciding what to do about a
//! failure (fall back to `rts`/`gpio`, or sleep-and-settle) lives in the caller
//! (`impl_host_serial.rs`/`impl_rs485.rs`), not here.
//!
//! Two raw calls, neither of which has a safe wrapper anywhere in this workspace's dependency
//! graph:
//!  * `ioctl(fd, TIOCSRS485, &mut serial_rs485)` — the kernel's native RS-485 hardware-timed
//!    driver-enable (`<linux/serial.h>` / `<asm-generic/ioctls.h>`).
//!  * `tcdrain(fd)` — POSIX `tcdrain(3)`: block until every byte handed to the fd has actually
//!    left the UART's TX FIFO. `RtsSoftware`'s drain step needs this specifically (not just an
//!    async `flush`, which only empties tokio-serial's own buffer into the fd — it says nothing
//!    about whether the UART hardware has finished shifting the last byte out yet).
//!
//! No `libc`/`nix` dependency: both are declared here as minimal hand-rolled `extern "C"`
//! bindings (both are plain, stable libc symbols; `ioctl`'s C signature is variadic, which stable
//! Rust can declare and call in an `extern` block).
//!
//! VERIFIED: `cargo build -p lib-io --features host` compiles this file, and
//! `tests/direction_mode.rs`'s `kernel_rs485_fails_open_loudly_naming_fallback_modes_on_a_pty`
//! exercises `enable_kernel_rs485`'s ENOTTY path for real against a pty (independently confirmed
//! with a standalone raw-ioctl probe before writing that test — see its module doc). `drain`'s
//! `tcdrain` call is also exercised for real (a pty DOES support plain termios/line-discipline
//! calls like `tcdrain`, unlike the modem-control ioctls RTS toggling needs — same module doc).
//! `TIOCSRS485 = 0x542F` and the `serial_rs485` layout are reproduced from
//! `<asm-generic/ioctls.h>`/`<linux/serial.h>` recollection, not a verified header dump, but the
//! ENOTTY-on-a-pty behavior confirms the ioctl NUMBER at least reaches the tty layer correctly (a
//! wrong request number could plausibly also return ENOTTY, so this is not a full proof of the
//! exact flag/struct layout — only real hardware proves that).
//!
//! Field task: `TIOCSRS485` SUCCESS (not just the ENOTTY-on-a-pty failure path proven in CI) can
//! only be verified against a real RS-485 UART on the shipping HAT.
#![allow(unsafe_code)]

use std::io;
use std::os::raw::{c_int, c_ulong};
use std::os::unix::io::RawFd;

/// `TIOCSRS485` from `<asm-generic/ioctls.h>` — shared across every Linux arch relevant here
/// (x86/x86_64/arm/aarch64 all use the asm-generic ioctl numbering for this one).
const TIOCSRS485: c_ulong = 0x542F;

/// `SER_RS485_ENABLED` (bit 0, `<linux/serial.h>`) — turn on RS-485 mode.
const SER_RS485_ENABLED: u32 = 1 << 0;
/// `SER_RS485_RTS_ON_SEND` (bit 1) — drive RTS high (DE asserted) while transmitting; mirrors
/// `DirectionMode::RtsSoftware`'s own active-high convention (`impl_rs485.rs`'s
/// `write_request_to_send(true)` on `Direction::Transmit`) so `rts`->`kernel` is a drop-in swap.
const SER_RS485_RTS_ON_SEND: u32 = 1 << 1;

/// Mirrors the kernel's `struct serial_rs485` (`<linux/serial.h>`) field-for-field: three `u32`s
/// then five `u32`s of reserved padding. `repr(C)` so the layout matches exactly what the ioctl
/// expects; the kernel reads `flags`/the two delay fields and writes the (possibly clamped) result
/// back into the same struct, which we don't otherwise inspect.
#[repr(C)]
struct SerialRs485 {
    flags: u32,
    delay_rts_before_send: u32,
    delay_rts_after_send: u32,
    padding: [u32; 5],
}

extern "C" {
    fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
    fn tcdrain(fd: c_int) -> c_int;
}

/// Enable the kernel's native RS-485 driver-enable on `fd`, asserting DE (RTS) for the duration of
/// each transmit. One-shot: called once from `HostSerial::open_with_direction` for
/// `DirectionMode::KernelRs485` ports; never called again for the life of the port.
///
/// # Errors
/// The raw `io::Error` from the failed ioctl (via `io::Error::last_os_error`). `ENOTTY`/`EINVAL`
/// specifically mean the UART/driver does not implement `TIOCSRS485` at all (common on USB-serial
/// adapters and generic UARTs without RS-485 hardware support, and — usefully for CI — on every
/// pty, which is how `tests/direction_mode.rs` proves this failure path without real hardware).
/// The caller (not this function — see the module doc's "keep the FFI/C shim DUMB" note) must fail
/// `open()` loudly, naming the `rts`/`gpio` fallback modes.
pub(super) fn enable_kernel_rs485(fd: RawFd) -> io::Result<()> {
    let mut req = SerialRs485 {
        flags: SER_RS485_ENABLED | SER_RS485_RTS_ON_SEND,
        delay_rts_before_send: 0,
        delay_rts_after_send: 0,
        padding: [0; 5],
    };
    // SAFETY: `fd` is a live, open serial-TTY file descriptor for the duration of this call (the
    // caller's contract — it is not retained afterward). `req` is a validly-initialized,
    // `repr(C)`-laid-out `SerialRs485` whose address we pass as the ioctl's untyped third
    // (`void*`) argument; the kernel both reads it and writes back into it, and `req` stays valid
    // (on this function's stack) for the whole call.
    let rc = unsafe { ioctl(fd, TIOCSRS485, &mut req as *mut SerialRs485) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// `tcdrain(3)`: block until every byte already written to `fd` has actually left the UART's TX
/// FIFO. Used by `DirectionMode::RtsSoftware`'s drain step (`impl_rs485.rs`) between the async
/// `flush` (which only empties tokio-serial's own write buffer into the fd) and dropping DE.
///
/// # Errors
/// The raw `io::Error` from the failed syscall.
pub(super) fn drain(fd: RawFd) -> io::Result<()> {
    // SAFETY: `fd` is a live, open file descriptor for the duration of this call (the caller's
    // contract). `tcdrain` takes no pointer arguments — there is no buffer/layout contract beyond
    // the fd itself being valid.
    let rc = unsafe { tcdrain(fd) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
