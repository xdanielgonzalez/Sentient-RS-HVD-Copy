/// RS-485 half-duplex driver-enable (DE/RE) mechanism, selected per port at
/// [`super::HostSerial::open_with_direction`] time. Three mechanisms, in order of how much of the
/// transceiver's timing they hand to hardware:
///
/// * [`DirectionMode::RtsSoftware`] — userspace toggles the UART's RTS line around each transmit
///   (the only mechanism this crate had before this workstream). **The default**: today's
///   behavior, PLUS the TX-drain (`flush` + `tcdrain`) + settle-time fix this workstream adds — a
///   bus that drops DE the instant the last byte is *handed to the kernel* (rather than once it has
///   actually left the UART's TX FIFO) truncates the frame on the wire. See `impl_rs485.rs`.
/// * [`DirectionMode::KernelRs485`] — the kernel's native `TIOCSRS485` hardware-timed
///   driver-enable (`rs485_ioctl.rs`), configured ONCE at `open()`. After that, `set_direction` is
///   a no-op: the UART/driver toggles DE itself around each transmit, with no software involvement
///   (and thus none of `RtsSoftware`'s drain/settle logic applies or is needed).
/// * [`DirectionMode::GpioDe`] — a dedicated GPIO line drives DE/RE (`gpio_de.rs`, via the
///   `gpiocdev` crate / Linux GPIO character-device uAPI), for HATs that wire DE to a spare GPIO
///   instead of to RTS. Its `Receive` transition drains the TX FIFO the same way `RtsSoftware`
///   does (`flush` + `tcdrain`, on the raw fd — see `impl_rs485.rs`'s `Rs485::drain_tx`) before
///   dropping DE, since a GPIO-driven DE line gates the same UART TX FIFO and is subject to the
///   same truncation failure mode. Unlike `RtsSoftware`, this drain is unconditional: `GpioDe`
///   carries no `drain: bool` field because (unlike `RtsSoftware`'s toggle, kept mainly so the
///   drain step itself can be disabled for debugging) there is no reason to make it optional here.
///
/// PROVISIONAL: `KernelRs485` and `GpioDe` are hardware-bound — neither has a pty equivalent
/// (`tests/direction_mode.rs` proves `RtsSoftware` end-to-end and proves `KernelRs485` fails open()
/// loudly on a pty, since a pty predictably has no `TIOCSRS485` support, but cannot prove either
/// mechanism *succeeds* without the real UART/GPIO hardware). **Field task: verify both on the
/// shipping HAT before enabling either in production config** — see this crate's `host` module doc
/// comment.
///
/// The default mirrors `bin-rpi`'s unconfigured `[serial]` TOML section — drain TX before
/// dropping DE, 5 ms assert-to-write settle, 10 ms write-to-deassert settle:
///
/// ```rust
/// use lib_io::host::DirectionMode;
///
/// assert_eq!(
///     DirectionMode::default(),
///     DirectionMode::RtsSoftware {
///         drain: true,
///         settle_tx_us: 5_000,
///         settle_rx_us: 10_000,
///     },
/// );
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum DirectionMode {
    /// Userspace RTS toggling — the historical/default mechanism.
    RtsSoftware {
        /// Flush the async write buffer, then `tcdrain(2)` the fd, before dropping DE (deasserting
        /// RTS). Without this, DE can drop (and the far end can start driving the bus) before the
        /// last byte has actually left the UART's TX FIFO, truncating the frame on the wire.
        drain: bool,
        /// Delay after asserting DE (RTS high), before the caller's write, in microseconds — lets
        /// the transceiver's driver-enable settle before data starts.
        settle_tx_us: u32,
        /// Delay after the (optionally drained) write completes, before deasserting DE, in
        /// microseconds — lets the last bit finish propagating on the bus before the transceiver
        /// goes back to receive-only.
        settle_rx_us: u32,
    },
    /// The kernel's native `TIOCSRS485` driver-enable, configured once at `open()`. `set_direction`
    /// is a no-op after that — the UART/driver owns DE toggling from here.
    KernelRs485,
    /// A dedicated GPIO line drives DE/RE (via `gpiocdev`).
    GpioDe {
        /// GPIO chip device, e.g. `"gpiochip4"` or `"/dev/gpiochip4"` (either form accepted — see
        /// `gpio_de::chip_path`).
        chip: String,
        /// GPIO line offset on `chip`, e.g. `22`.
        line: u32,
        /// Whether asserting DE means driving the line electrically high (`true`) or low
        /// (`false`, e.g. an active-low DE input).
        active_high: bool,
        /// Delay after asserting DE, before the caller's write, in microseconds.
        settle_tx_us: u32,
        /// Delay after the write completes, before deasserting DE, in microseconds.
        settle_rx_us: u32,
    },
}
