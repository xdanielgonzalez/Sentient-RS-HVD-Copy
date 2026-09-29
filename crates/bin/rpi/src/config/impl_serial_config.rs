use super::prelude::*;

impl SerialConfig {
    /// Serial line parameters for the io factory. Per-bus (moved off `Config` in #238/#242 —
    /// each `BusConfig`'s own `SerialConfig` carries its line params independently).
    pub fn serial_params(&self) -> SerialParams {
        SerialParams {
            baud: self.baud,
            data_bits: 8,
            parity: match self.parity {
                ParityCfg::None => Parity::None,
                ParityCfg::Even => Parity::Even,
                ParityCfg::Odd => Parity::Odd,
            },
            stop_bits: if self.stop_bits >= 2 {
                StopBits::Two
            } else {
                StopBits::One
            },
        }
    }

    pub fn serial_kind(&self) -> SerialKind {
        match self.kind {
            SerialKindCfg::Rs232 => SerialKind::Rs232,
            SerialKindCfg::Rs485 => SerialKind::Rs485,
        }
    }

    /// Resolve this port's RS-485 driver-enable mechanism into the concrete
    /// `lib_io::host::DirectionMode` that `HostSerial::open_with_direction` consumes, folding in
    /// this port's `gpio_*`/`settle_*_us`/`tx_drain` fields.
    ///
    /// Precedence: `self.direction` (this port's own `[serial]`/`[[bus]].serial` override) wins
    /// over `top_level` (the process-wide `[direction]` selector); if BOTH are absent, the
    /// default is `DirectionCfg::default()` (`"rts"`, i.e. `DirectionMode::RtsSoftware`).
    ///
    /// Fails config load (`Err`, never a silent fallback) when `direction = "gpio"` is selected
    /// (by either surface) without both `gpio_chip` and `gpio_line` set — a GPIO DE/RE line has no
    /// sane default to fall back to.
    pub fn direction_mode(&self, top_level: Option<DirectionCfg>) -> anyhow::Result<DirectionMode> {
        let selected = self.direction.or(top_level).unwrap_or_default();
        Ok(match selected {
            DirectionCfg::Rts => DirectionMode::RtsSoftware {
                drain: self.tx_drain,
                settle_tx_us: self.settle_tx_us,
                settle_rx_us: self.settle_rx_us,
            },
            DirectionCfg::Tiocsrs485 => DirectionMode::KernelRs485,
            DirectionCfg::Gpio => DirectionMode::GpioDe {
                chip: self
                    .gpio_chip
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("direction = \"gpio\" requires gpio_chip"))?,
                line: self
                    .gpio_line
                    .ok_or_else(|| anyhow::anyhow!("direction = \"gpio\" requires gpio_line"))?,
                active_high: self.gpio_active_high,
                settle_tx_us: self.settle_tx_us,
                settle_rx_us: self.settle_rx_us,
            },
        })
    }

    /// Reject a `baud` of `0`, a `stop_bits` outside `{1, 2}`, or a `read_timeout_ms` of `0`.
    ///
    /// - `baud = 0`: not a serial line rate any hardware speaks.
    /// - `stop_bits` outside `{1, 2}`: [`SerialConfig::serial_params`] maps any `stop_bits >= 2` to
    ///   [`StopBits::Two`] and anything else (including `0`) to [`StopBits::One`] — a value like
    ///   `stop_bits = 5` parses fine but silently becomes `Two`, not the nonsense value entered.
    ///   This closes that silent-coercion gap.
    /// - `read_timeout_ms = 0`: `HostSerial::open_with_direction` builds
    ///   `Duration::from_millis(0)` as the per-exchange read deadline — every read then times out
    ///   before a single byte can physically arrive back from the bus, so the poll loop gets
    ///   `IoError::Timeout` every cycle forever, indistinguishable from a genuinely dead bus.
    ///
    /// Fails loudly rather than clamping — matching this config layer's established posture (see
    /// `PollConfig::validate`/`DdsToml::validate`).
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.baud == 0 {
            anyhow::bail!("config error: [serial].baud = 0 is not a usable serial line rate");
        }
        if self.stop_bits != 1 && self.stop_bits != 2 {
            anyhow::bail!(
                "config error: [serial].stop_bits = {} is not 1 or 2 (it would silently be \
                 coerced to {} by SerialConfig::serial_params, not rejected)",
                self.stop_bits,
                if self.stop_bits >= 2 { 2 } else { 1 }
            );
        }
        if self.read_timeout_ms == 0 {
            anyhow::bail!(
                "config error: [serial].read_timeout_ms = 0 times out every read before a byte \
                 can physically arrive — every poll cycle would fail forever"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
// Test code may unwrap/panic; the workspace lint gate denies these in production logic only.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn cfg(baud: u32, stop_bits: u8, read_timeout_ms: u32) -> SerialConfig {
        SerialConfig {
            kind: SerialKindCfg::Rs485,
            port: "/dev/ttyAMA0".to_string(),
            baud,
            parity: ParityCfg::None,
            stop_bits,
            read_timeout_ms,
            direction: None,
            gpio_chip: None,
            gpio_line: None,
            gpio_active_high: true,
            settle_tx_us: 5_000,
            settle_rx_us: 10_000,
            tx_drain: true,
        }
    }

    #[test]
    fn validate_rejects_zero_baud() {
        assert!(cfg(0, 1, 1000).validate().is_err());
    }

    #[test]
    fn validate_accepts_the_default_baud() {
        assert!(cfg(19200, 1, 1000).validate().is_ok());
    }

    #[test]
    fn validate_rejects_stop_bits_outside_one_or_two() {
        assert!(cfg(19200, 3, 1000).validate().is_err());
        assert!(cfg(19200, 0, 1000).validate().is_err());
    }

    #[test]
    fn validate_accepts_stop_bits_one_and_two() {
        assert!(cfg(19200, 1, 1000).validate().is_ok());
        assert!(cfg(19200, 2, 1000).validate().is_ok());
    }

    #[test]
    fn validate_rejects_zero_read_timeout_ms() {
        assert!(cfg(19200, 1, 0).validate().is_err());
    }

    #[test]
    fn validate_accepts_the_smallest_nonzero_read_timeout_ms() {
        assert!(cfg(19200, 1, 1).validate().is_ok());
    }
}
