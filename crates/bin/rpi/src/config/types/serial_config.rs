use super::direction_cfg::DirectionCfg;
use super::parity_cfg::ParityCfg;
use super::serial_kind_cfg::SerialKindCfg;
use serde::Deserialize;

/// One `[serial]`/`[[bus]].serial` section. The `direction`/`gpio_*`/`settle_*_us`/`tx_drain`
/// fields (#245) are the per-port mirror of `lib_io::host::DirectionMode` — see
/// `DirectionCfg`'s doc comment for how the two config surfaces (this one, and the pre-existing
/// top-level `[direction]`) relate. `direction` here is a per-port OVERRIDE (`None` means "use
/// the top-level `[direction]` selector, or the hard-coded default if that's absent too" — see
/// [`SerialConfig::direction_mode`], which resolves the precedence and maps to the concrete
/// `lib_io::host::DirectionMode` (folding in `gpio_*`/`settle_*_us`/`tx_drain`) that
/// `bin_rpi_platform::HostPlatform::build_port`/`HostSerial::open_with_direction` actually consume.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SerialConfig {
    /// `rs232` or `rs485`.
    pub kind: SerialKindCfg,
    /// OS serial device path (e.g. `/dev/ttyAMA0`).
    pub port: String,
    /// Serial line baud rate. Must be non-zero — see [`SerialConfig::validate`]. The actual rate
    /// is left otherwise unbounded (a nonstandard rate can be legitimate — Linux termios can be
    /// configured for a custom `BOTHER` rate on some hardware), only `0` is degenerate.
    #[serde(default = "default_baud")]
    pub baud: u32,
    #[serde(default)]
    pub parity: ParityCfg,
    /// Stop bits: `1` or `2`. Anything else currently parses fine but silently maps to `Two`
    /// whenever `>= 2` (see `SerialConfig::serial_params`) or `One` otherwise — see
    /// [`SerialConfig::validate`], which now rejects that silent coercion instead.
    #[serde(default = "default_stop_bits")]
    pub stop_bits: u8,
    /// Per-exchange read deadline, in milliseconds. Must be non-zero — see
    /// [`SerialConfig::validate`].
    #[serde(default = "default_read_timeout_ms")]
    pub read_timeout_ms: u32,
    /// RS-485 driver-enable mechanism for THIS port: `"rts"` (software RTS toggling),
    /// `"kernel"`/`"tiocsrs485"` (the kernel's native `TIOCSRS485`), or `"gpio"` (a dedicated
    /// DE/RE line — see `gpio_chip`/`gpio_line`/`gpio_active_high`). Ignored for `kind = "rs232"`.
    /// `None` (the default — this field is not required) falls back to the top-level
    /// `[direction]` selector, and then to `DirectionCfg::default()` (`"rts"`) if that's absent
    /// too — see [`SerialConfig::direction_mode`].
    #[serde(default)]
    pub direction: Option<DirectionCfg>,
    /// GPIO chip for `direction = "gpio"`, e.g. `"gpiochip4"` (bare name) or `"/dev/gpiochip4"`
    /// (full path — both accepted). Required when `direction = "gpio"` (checked by
    /// [`SerialConfig::direction_mode`], which fails config load if absent); otherwise unused.
    #[serde(default)]
    pub gpio_chip: Option<String>,
    /// GPIO line offset on `gpio_chip` for `direction = "gpio"`, e.g. `22`. Required when
    /// `direction = "gpio"` (see `gpio_chip`).
    #[serde(default)]
    pub gpio_line: Option<u32>,
    /// Whether asserting DE means driving `gpio_line` electrically high (`true`, the default) or
    /// low (`false`). Only meaningful for `direction = "gpio"`.
    #[serde(default = "default_gpio_active_high")]
    pub gpio_active_high: bool,
    /// Delay after asserting DE, before writing, in microseconds. Applies to `direction = "rts"`
    /// and `direction = "gpio"` (a no-op for `"kernel"`, which the hardware/kernel times itself).
    #[serde(default = "default_settle_tx_us")]
    pub settle_tx_us: u32,
    /// Delay after the write completes, before deasserting DE, in microseconds. Same scope as
    /// `settle_tx_us`.
    #[serde(default = "default_settle_rx_us")]
    pub settle_rx_us: u32,
    /// Flush + `tcdrain` the TX buffer before dropping DE (`direction = "rts"` only — see
    /// `lib_io::host::DirectionMode::RtsSoftware`'s doc comment for why this fixes RS-485 frame
    /// truncation). Default `true`; only ever worth disabling to debug the drain step itself.
    #[serde(default = "default_tx_drain")]
    pub tx_drain: bool,
}

fn default_baud() -> u32 {
    19200
}
fn default_stop_bits() -> u8 {
    1
}
fn default_read_timeout_ms() -> u32 {
    1000
}
fn default_gpio_active_high() -> bool {
    true
}
fn default_settle_tx_us() -> u32 {
    5_000
}
fn default_settle_rx_us() -> u32 {
    10_000
}
fn default_tx_drain() -> bool {
    true
}
