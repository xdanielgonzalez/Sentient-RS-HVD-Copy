use super::gpio_de;
use super::prelude::*;
use super::rs485_ioctl;

impl HostSerial {
    /// Open `path` with `params` as `kind`, using [`DirectionMode::default`] for RS-485 direction
    /// control (software RTS toggling, TX-drained). Every existing caller of this function gets
    /// the TX-drain/settle fix automatically, with no call-site change — see
    /// [`HostSerial::open_with_direction`] for the full three-mode seam this workstream adds.
    ///
    /// Returns an [`OpenError`] carrying the OS/serial detail (so an absent device or a
    /// permissions problem is diagnosable), rather than a detail-free `IoError::Device`.
    ///
    /// Line-parameter validation runs BEFORE any device is touched, so a bad config fails
    /// naming the offending field instead of surfacing as an OS error against the tty:
    ///
    /// ```rust
    /// use lib_io::SerialParams;
    /// use lib_io::host::{HostSerial, OpenError, SerialKind};
    ///
    /// // Only 5..=8 data bits are representable; the path is never opened.
    /// let params = SerialParams { data_bits: 4, ..SerialParams::default() };
    /// let res = HostSerial::open("/dev/ttyUSB0", SerialKind::Rs232, params, 500);
    /// assert!(matches!(res, Err(OpenError::InvalidParams(msg)) if msg.contains("data_bits")));
    /// ```
    pub fn open(
        path: &str,
        kind: SerialKind,
        params: SerialParams,
        read_timeout_ms: u32,
    ) -> Result<Self, OpenError> {
        Self::open_with_direction(
            path,
            kind,
            params,
            read_timeout_ms,
            DirectionMode::default(),
        )
    }

    /// Open `path` with `params` as `kind`, using an explicit RS-485 [`DirectionMode`] (ignored for
    /// [`SerialKind::Rs232`], which is always full-duplex — `set_direction` is a no-op there
    /// regardless). See this crate's `host` module doc comment for the three-mode seam.
    ///
    /// Returns [`OpenError::Direction`] specifically when a `KernelRs485`/`GpioDe` mechanism could
    /// not be initialized (naming the `rts`/other fallback modes) — see [`OpenError`]'s doc
    /// comment. Direction-mechanism setup happens BEFORE the port is handed back, so a caller never
    /// receives an [`Rs485`] whose configured direction mode silently failed to initialize.
    pub fn open_with_direction(
        path: &str,
        kind: SerialKind,
        params: SerialParams,
        read_timeout_ms: u32,
        direction: DirectionMode,
    ) -> Result<Self, OpenError> {
        let read_timeout = Duration::from_millis(read_timeout_ms as u64);
        // Build the serial port configuration
        let mut port = tokio_serial::new(path, params.baud);

        // Set data bits
        port = port.data_bits(match params.data_bits {
            5 => tokio_serial::DataBits::Five,
            6 => tokio_serial::DataBits::Six,
            7 => tokio_serial::DataBits::Seven,
            8 => tokio_serial::DataBits::Eight,
            _ => return Err(OpenError::InvalidParams("data_bits must be 5..=8")),
        });

        // Set parity
        port = port.parity(match params.parity {
            Parity::None => tokio_serial::Parity::None,
            Parity::Even => tokio_serial::Parity::Even,
            Parity::Odd => tokio_serial::Parity::Odd,
        });

        // Set stop bits
        port = port.stop_bits(match params.stop_bits {
            StopBits::One => tokio_serial::StopBits::One,
            StopBits::Two => tokio_serial::StopBits::Two,
        });

        // Open the port
        let stream = port.open_native_async().map_err(OpenError::Io)?;

        // Build the appropriate variant
        match kind {
            SerialKind::Rs232 => Ok(HostSerial::Rs232(Rs232 {
                stream,
                read_timeout,
            })),
            SerialKind::Rs485 => {
                let gpio_line = init_direction(&stream, &direction)?;
                Ok(HostSerial::Rs485(Rs485 {
                    stream,
                    read_timeout,
                    direction,
                    gpio_line,
                }))
            }
        }
    }

    /// A placeholder transport for when the configured port could not be opened. Used by
    /// [`crate::host`]'s platform build/reconnect so the poll loop starts (or keeps running)
    /// DEGRADED rather than the process exiting or hard-erroring — see [`HostSerial::Disconnected`].
    ///
    /// Every read/write fails fast with [`IoError::Timeout`] — the same error a silent/absent
    /// device already surfaces — and direction control stays a no-op `Ok` (as on RS-232), so
    /// the poll loop's existing dead-cycle/backoff/reconnect machinery is the only recovery
    /// path, with no new failure mode to special-case:
    ///
    /// ```rust
    /// use lib_io::host::HostSerial;
    /// use lib_io::{Direction, IoError, SerialTransport};
    ///
    /// # fn main() -> Result<(), IoError> {
    /// # let rt = tokio::runtime::Builder::new_current_thread()
    /// #     .build()
    /// #     .map_err(|_| IoError::Device)?;
    /// # rt.block_on(async {
    /// let mut port = HostSerial::disconnected();
    /// let mut buf = [0u8; 32];
    ///
    /// assert_eq!(port.read(&mut buf).await, Err(IoError::Timeout));
    /// assert_eq!(port.read_until(b'\r', &mut buf).await, Err(IoError::Timeout));
    /// assert_eq!(port.write_all(b"~REQ\r").await, Err(IoError::Timeout));
    /// assert_eq!(port.set_direction(Direction::Transmit).await, Ok(()));
    /// # Ok(())
    /// # })
    /// # }
    /// ```
    pub fn disconnected() -> Self {
        HostSerial::Disconnected
    }
}

/// One-shot direction-mechanism setup performed ONCE at open (never on a later `set_direction`
/// call): configures `TIOCSRS485` for [`DirectionMode::KernelRs485`], or requests (deasserted) the
/// GPIO line for [`DirectionMode::GpioDe`] — returned so it can be held on the `Rs485` for the
/// port's lifetime. A no-op for [`DirectionMode::RtsSoftware`] (nothing to set up ahead of time;
/// RTS is toggled per-transmit in `Rs485::set_direction`).
fn init_direction(
    stream: &tokio_serial::SerialStream,
    direction: &DirectionMode,
) -> Result<Option<gpiocdev::Request>, OpenError> {
    match direction {
        DirectionMode::RtsSoftware { .. } => Ok(None),
        DirectionMode::KernelRs485 => {
            rs485_ioctl::enable_kernel_rs485(stream.as_raw_fd()).map_err(|e| {
                OpenError::Direction(format!(
                    "TIOCSRS485 not supported by this UART/driver ({e}); fall back to \
                     direction = \"rts\" (software RTS toggling) or \"gpio\" (a dedicated DE/RE \
                     line)"
                ))
            })?;
            Ok(None)
        }
        DirectionMode::GpioDe {
            chip,
            line,
            active_high,
            ..
        } => {
            let req = gpio_de::open_line(chip, *line, *active_high).map_err(|e| {
                OpenError::Direction(format!(
                    "GPIO DE line {chip}:{line} could not be requested ({e}); fall back to \
                     direction = \"rts\" (software RTS toggling) or \"kernel\" (TIOCSRS485, if \
                     the UART/driver supports it)"
                ))
            })?;
            Ok(Some(req))
        }
    }
}

impl SerialTransport for HostSerial {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, IoError> {
        match self {
            HostSerial::Rs232(r) => r.read(buf).await,
            HostSerial::Rs485(r) => r.read(buf).await,
            HostSerial::Disconnected => Err(IoError::Timeout),
        }
    }

    async fn read_until(&mut self, delimiter: u8, buf: &mut [u8]) -> Result<usize, IoError> {
        match self {
            HostSerial::Rs232(r) => r.read_until(delimiter, buf).await,
            HostSerial::Rs485(r) => r.read_until(delimiter, buf).await,
            HostSerial::Disconnected => Err(IoError::Timeout),
        }
    }

    async fn write_all(&mut self, bytes: &[u8]) -> Result<(), IoError> {
        match self {
            HostSerial::Rs232(r) => r.write_all(bytes).await,
            HostSerial::Rs485(r) => r.write_all(bytes).await,
            HostSerial::Disconnected => Err(IoError::Timeout),
        }
    }

    async fn set_direction(&mut self, dir: Direction) -> Result<(), IoError> {
        match self {
            HostSerial::Rs232(r) => r.set_direction(dir).await,
            HostSerial::Rs485(r) => r.set_direction(dir).await,
            HostSerial::Disconnected => Ok(()),
        }
    }

    /// `Disconnected` is a no-op `Ok`, matching `set_direction`'s treatment: there is no live
    /// port to clear, and this isn't a new failure mode for the poll loop's reconnect machinery
    /// to special-case.
    async fn discard_input(&mut self) -> Result<(), IoError> {
        match self {
            HostSerial::Rs232(r) => r.discard_input().await,
            HostSerial::Rs485(r) => r.discard_input().await,
            HostSerial::Disconnected => Ok(()),
        }
    }
}
