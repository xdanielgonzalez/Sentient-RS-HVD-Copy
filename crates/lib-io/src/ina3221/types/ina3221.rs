use lib_canonical::DeviceRail;

/// Async driver for the TI **INA3221** triple-channel, high-side shunt/bus-voltage monitor —
/// measures the adapter board's OWN rail draw (diagnostics/UI only; NOT the project's power
/// source, which is the Seplos BMS — see [`crate::ina3221`]'s own module doc).
///
/// Generic over `I2C: embedded_hal_async::i2c::I2c` **only** — no `embassy-*` dependency, no
/// digital/GPIO trait, no delay trait. This mirrors [`crate::embassy_uart::EmbassyUart`]'s
/// embassy-version-free construction (see that module's doc comment for why): the board crate
/// supplies whichever concrete I2C peripheral/bus-sharing wrapper it likes — on the bench this is
/// an `embedded-hal-bus` (`async`) `I2cDevice` sharing I2C1 with the OLED FeatherWing, which is
/// exactly why this driver had to be async in the first place (mixing a blocking wrapper with an
/// async one on one bus is a trait-bound trap — `ssd1306` issue #215).
///
/// `rails` maps each of the INA3221's 3 physical channels (array index `0..=2` == datasheet
/// "Channel 1".."Channel 3") to the [`DeviceRail`] it is wired to; `None` marks an
/// unpopulated/spare channel. [`crate::ina3221::read_all`] skips a `None` channel entirely —
/// never writing an entry for it — matching `spec_app::RailMonitor::read_all`'s exact contract
/// (its doc comment: "A channel with no rail wired ... is simply not written").
pub struct Ina3221<I2C> {
    pub(crate) i2c: I2C,
    /// 7-bit I2C target address (`0x40` default; `0x41`/`0x42`/`0x43` via the `A0` pin — TI
    /// INA3221 datasheet SBOS576C, Table 7-1).
    pub(crate) address: u8,
    pub(crate) rails: [Option<DeviceRail>; 3],
}
