//! Host (RPi Linux) serial transports over `tokio-serial` — the paid deliverable path.
//!
//! `Rs232` and `Rs485` both implement [`crate::transport::SerialTransport`]; `HostSerial` is an **enum factory**
//! that selects the right one from config **without boxing** (static dispatch via enum-delegation).
//!
//! RS-485 direction control is a three-mode seam (see [`DirectionMode`], selected at
//! [`HostSerial::open_with_direction`]):
//!  * software RTS toggling (the DEFAULT — [`DirectionMode::RtsSoftware`]), TX-drained
//!    (`flush` + `tcdrain`) before dropping DE, with configurable pre/post settle delays;
//!  * the kernel's native `TIOCSRS485` hardware-timed driver-enable
//!    ([`DirectionMode::KernelRs485`]), configured once at `open()`, after which `set_direction`
//!    is a no-op — the UART/driver owns DE toggling from there;
//!  * a dedicated GPIO DE/RE line ([`DirectionMode::GpioDe`]), via the `gpiocdev` crate, for HATs
//!    that don't wire DE to RTS at all.
//!
//! This crate's one unsafe call site — the `TIOCSRS485` ioctl plus the `tcdrain` syscall — lives
//! in `rs485_ioctl` (see that module's doc comment for why, and why it's the crate's only
//! `#[allow(unsafe_code)]`); GPIO control (`gpio_de`) is 100% safe Rust over `gpiocdev`.
//!
//! Field task: all three modes are ultimately hardware-bound for a real "it transmits correctly"
//! proof — even `RtsSoftware`'s `Transmit` side can't be exercised over a pty (a Linux pty doesn't
//! implement the modem-control ioctls `write_request_to_send` needs; only its `Receive`-side
//! drain/settle SEQUENCING is pty-provable). Verify all three on the shipping HAT before enabling
//! any of them in production config. See `tests/direction_mode.rs`'s module doc for exactly what
//! CI *can* and can't prove here.

mod prelude;
mod types;

mod gpio_de;
mod impl_direction_mode;
mod impl_host_serial;
mod impl_open_error;
mod impl_rs232;
mod impl_rs485;
mod rs485_ioctl;

pub use types::*;

#[cfg(test)]
mod tests {
    //! `HostSerial::open`'s parameter validation runs *before* the port is ever touched (the
    //! `data_bits` match short-circuits ahead of `open_native_async`), and a bad path is rejected
    //! by the OS on every platform — both are exercised here without a real or virtual serial
    //! device, alongside the pure/no-I/O `OpenError`/`DirectionMode` logic (`Display` text,
    //! `DirectionMode::default()`'s settle values). Everything hardware/pty-bound (`Rs232`/`Rs485`
    //! read/write, and RS-485 direction control specifically) is covered instead by the
    //! `host_pty`/`host_timeout` integration tests and the `direction_mode` integration test
    //! (`#[cfg(unix)]`, run on the Linux CI container — a pty has no Windows equivalent).
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::transport::{Direction, IoError, Parity, SerialParams, SerialTransport, StopBits};

    #[test]
    fn invalid_data_bits_is_rejected_before_opening_any_port() {
        let params = SerialParams {
            baud: 19_200,
            data_bits: 4, // only 5..=8 are valid RS-232/RS-485 word sizes
            parity: Parity::None,
            stop_bits: StopBits::One,
        };
        // The path is never touched: validation happens before the port would be opened.
        let res = HostSerial::open("this-path-is-never-touched", SerialKind::Rs232, params, 100);
        let err = match res {
            Ok(_) => panic!("out-of-range data_bits must be rejected"),
            Err(e) => e,
        };
        match err {
            OpenError::InvalidParams(msg) => assert!(msg.contains("data_bits")),
            OpenError::Io(e) => panic!("expected InvalidParams, got Io({e})"),
            OpenError::Direction(d) => panic!("expected InvalidParams, got Direction({d})"),
        }
    }

    #[test]
    fn open_error_display_carries_detail_for_a_missing_device() {
        let res = HostSerial::open(
            "/dev/tty-sentient-does-not-exist-anywhere",
            SerialKind::Rs232,
            SerialParams::default(),
            100,
        );
        let err = match res {
            Ok(_) => panic!("a nonexistent device must fail to open"),
            Err(e) => e,
        };
        match err {
            OpenError::Io(_) => {
                let msg = format!("{err}");
                assert!(!msg.is_empty(), "Display must carry the OS/serial detail");
            }
            OpenError::InvalidParams(p) => panic!("unexpected InvalidParams: {p}"),
            OpenError::Direction(d) => panic!("unexpected Direction: {d}"),
        }
    }

    #[test]
    fn open_error_invalid_params_display_names_the_field() {
        let err = OpenError::InvalidParams("data_bits must be 5..=8");
        assert_eq!(
            format!("{err}"),
            "invalid serial parameter: data_bits must be 5..=8"
        );
    }

    #[test]
    fn open_error_direction_display_carries_the_fallback_mode_message() {
        let err = OpenError::Direction("TIOCSRS485 not supported; fall back to rts".to_string());
        assert_eq!(
            format!("{err}"),
            "RS-485 direction control: TIOCSRS485 not supported; fall back to rts"
        );
    }

    #[test]
    fn direction_mode_default_is_rts_software_with_the_documented_settle_defaults() {
        match DirectionMode::default() {
            DirectionMode::RtsSoftware {
                drain,
                settle_tx_us,
                settle_rx_us,
            } => {
                assert!(drain, "the default must drain TX before dropping DE");
                assert_eq!(settle_tx_us, 5_000);
                assert_eq!(settle_rx_us, 10_000);
            }
            other => panic!("default direction mode must be RtsSoftware, got {other:?}"),
        }
    }

    /// Serial-port startup resilience (host platform DEGRADED mode): `HostSerial::disconnected()`
    /// is the placeholder transport `HostPlatform::build`/`reconnect` fall back to when the
    /// configured port can't be opened. Every read/write must fail fast with `IoError::Timeout` —
    /// the same error the real transports already surface on a silent/absent device — so the run
    /// loop's existing dead-cycle/backoff/reconnect path drives recovery with no new failure mode
    /// to special-case.
    #[tokio::test]
    async fn disconnected_read_and_write_fail_with_timeout() {
        let mut s = HostSerial::disconnected();
        let mut buf = [0u8; 16];

        assert_eq!(s.read(&mut buf).await, Err(IoError::Timeout));
        assert_eq!(s.read_until(b'\r', &mut buf).await, Err(IoError::Timeout));
        assert_eq!(s.write_all(b"hello").await, Err(IoError::Timeout));
    }

    /// `set_direction` is a no-op on RS-232 too (see `Rs232`'s impl), so `Disconnected` matching
    /// that (rather than also failing) keeps it from being a spurious extra failure mode inside a
    /// `set_direction`-around-transmit call site that isn't itself checking for a live port.
    #[tokio::test]
    async fn disconnected_set_direction_is_a_no_op_ok() {
        let mut s = HostSerial::disconnected();
        assert_eq!(s.set_direction(Direction::Transmit).await, Ok(()));
    }
}
