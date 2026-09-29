use super::gpio_de;
use super::prelude::*;
use super::rs485_ioctl;

impl SerialTransport for Rs485 {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, IoError> {
        let dur = self.read_timeout;
        match timeout(dur, async {
            self.stream.read(buf).await.map_err(|_| IoError::Device)
        })
        .await
        {
            Ok(inner) => inner,
            Err(_elapsed) => Err(IoError::Timeout),
        }
    }

    async fn read_until(&mut self, delimiter: u8, buf: &mut [u8]) -> Result<usize, IoError> {
        let dur = self.read_timeout;
        match timeout(dur, async {
            let mut pos = 0;
            while pos < buf.len() {
                let mut byte = [0u8; 1];
                match self.stream.read_exact(&mut byte).await {
                    Ok(_) => {
                        buf[pos] = byte[0];
                        pos += 1;
                        if byte[0] == delimiter {
                            return Ok(pos);
                        }
                    }
                    Err(_) => return Err(IoError::Device),
                }
            }
            Err(IoError::ShortTransfer)
        })
        .await
        {
            Ok(inner) => inner,
            Err(_elapsed) => Err(IoError::Timeout),
        }
    }

    async fn write_all(&mut self, bytes: &[u8]) -> Result<(), IoError> {
        let dur = self.read_timeout;
        match timeout(dur, async {
            self.stream
                .write_all(bytes)
                .await
                .map_err(|_| IoError::Device)
        })
        .await
        {
            Ok(inner) => inner,
            Err(_elapsed) => Err(IoError::Timeout),
        }
    }

    /// Direction control, dispatched on this port's configured [`DirectionMode`] (set once at
    /// open — see `HostSerial::open_with_direction`):
    ///  * `RtsSoftware` — toggle RTS; on the `Receive` transition (dropping DE), drain first
    ///    (`flush` + `tcdrain`) when `drain` is set, then sleep `settle_rx_us` before deasserting
    ///    — this is the TX-drain/settle fix this workstream adds (see the type's doc comment).
    ///    Asserting (`Transmit`) sleeps `settle_tx_us` afterward so the caller's write starts only
    ///    once the transceiver has settled into drive mode.
    ///  * `KernelRs485` — a no-op; the kernel/UART already toggles DE around each transmit (set up
    ///    once at open, see `rs485_ioctl::enable_kernel_rs485`).
    ///  * `GpioDe` — the GPIO-line equivalent of `RtsSoftware`'s assert/settle dance. On the
    ///    `Receive` transition it drains the same way `RtsSoftware` does (`flush` + `tcdrain` via
    ///    `Rs485::drain_tx`) before dropping DE: a GPIO-driven DE line still gates the same
    ///    UART TX FIFO, so the frame-truncation failure mode `RtsSoftware`'s drain fixes applies
    ///    here too. Unlike `RtsSoftware`, this is unconditional — `GpioDe` has no `drain: bool`
    ///    field (see the type's doc comment) because there is no debug/back-compat reason to make
    ///    it optional here; it is simply always correct.
    async fn set_direction(&mut self, dir: Direction) -> Result<(), IoError> {
        match &self.direction {
            DirectionMode::RtsSoftware {
                drain,
                settle_tx_us,
                settle_rx_us,
            } => {
                let (drain, settle_tx_us, settle_rx_us) = (*drain, *settle_tx_us, *settle_rx_us);
                match dir {
                    Direction::Transmit => {
                        self.stream
                            .write_request_to_send(true)
                            .map_err(|_| IoError::Direction)?;
                        sleep_us(settle_tx_us).await;
                        Ok(())
                    }
                    Direction::Receive => {
                        if drain {
                            self.drain_tx().await?;
                        }
                        sleep_us(settle_rx_us).await;
                        self.stream
                            .write_request_to_send(false)
                            .map_err(|_| IoError::Direction)
                    }
                }
            }
            // The kernel/UART owns DE toggling from `open()` onward — no software involvement.
            DirectionMode::KernelRs485 => Ok(()),
            DirectionMode::GpioDe {
                line,
                settle_tx_us,
                settle_rx_us,
                ..
            } => {
                let (line, settle_tx_us, settle_rx_us) = (*line, *settle_tx_us, *settle_rx_us);
                // `req` is (re-)borrowed from `self.gpio_line` separately in each arm below,
                // rather than once up front: the `Receive` arm's drain step needs `&mut self`
                // (`self.drain_tx()`), which would conflict with an already-live `&self.gpio_line`
                // borrow held across that call.
                match dir {
                    Direction::Transmit => {
                        let Some(req) = self.gpio_line.as_ref() else {
                            // Unreachable in practice: `open_with_direction` always requests the
                            // line for `GpioDe` before constructing this `Rs485`. Fail safe rather
                            // than panic.
                            return Err(IoError::Direction);
                        };
                        gpio_de::set(req, line, true).map_err(|_| IoError::Direction)?;
                        sleep_us(settle_tx_us).await;
                        Ok(())
                    }
                    Direction::Receive => {
                        // Same drain as `RtsSoftware`'s Receive transition (flush + tcdrain, on
                        // the raw fd, independent of which DE mechanism is in play) — dropping a
                        // GPIO DE line before the UART's TX FIFO has actually emptied truncates
                        // the frame on the wire exactly as it does for RTS-toggled DE.
                        self.drain_tx().await?;
                        sleep_us(settle_rx_us).await;
                        let Some(req) = self.gpio_line.as_ref() else {
                            // Unreachable in practice: see the `Transmit` arm above.
                            return Err(IoError::Direction);
                        };
                        gpio_de::set(req, line, false).map_err(|_| IoError::Direction)
                    }
                }
            }
        }
    }

    /// Clears the OS-level input buffer via `tokio-serial`'s `SerialPort::clear`, so a stale
    /// partial frame left over from a failed/timed-out exchange can't bleed into the next
    /// `read_until`. Direction-mode-independent — it operates on the underlying stream's input
    /// buffer regardless of which DE mechanism is configured.
    async fn discard_input(&mut self) -> Result<(), IoError> {
        self.stream
            .clear(ClearBuffer::Input)
            .map_err(|_| IoError::Device)
    }
}

impl Rs485 {
    /// The shared drain step behind both `RtsSoftware`'s and `GpioDe`'s `Receive` transitions:
    /// flush tokio-serial's own write buffer down to the fd, then `tcdrain(2)` the fd so this call
    /// doesn't return until the UART's TX FIFO has actually emptied — only after that is it safe
    /// to drop DE (however it's driven — RTS or a GPIO line) without truncating the frame on the
    /// wire. Operates on the raw fd (`rs485_ioctl::drain`), so it applies regardless of DE
    /// mechanism.
    async fn drain_tx(&mut self) -> Result<(), IoError> {
        self.stream.flush().await.map_err(|_| IoError::Device)?;
        let fd = self.stream.as_raw_fd();
        rs485_ioctl::drain(fd).map_err(|_| IoError::Device)
    }
}

/// Sleep for `us` microseconds, skipping the call entirely for `0` (the common case when a
/// direction mode's settle time is deliberately configured off, e.g. in a fast test).
async fn sleep_us(us: u32) {
    if us > 0 {
        sleep(Duration::from_micros(u64::from(us))).await;
    }
}
