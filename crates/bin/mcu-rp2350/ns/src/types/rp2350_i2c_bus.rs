/// The raw I2C1 peripheral in async mode (SDA=GP2, SCL=GP3 — Feather RP2350 fixed pins; see the
/// Feather pinout ground-truth note; also the STEMMA QT connector's bus) — directly owned by the
/// OLED FeatherWing leg alone (`boot_ui.rs::build_display` constructs it). The INA3221 (WP4) is wired
/// on the bench to a DIFFERENT peripheral, I2C0 (see [`super::rp2350_i2c0_bus::Rp2350I2c0Bus`]), so
/// this is no longer a bus shared between two consumers — see `boot_i2c.rs`'s module doc for why the
/// earlier shared-bus arrangement (a `Mutex`-guarded bus + per-consumer `I2cDevice` handles) was
/// removed rather than kept and just re-pointed.
#[cfg(feature = "ui")]
pub(crate) type Rp2350I2c1Bus =
    embassy_rp::i2c::I2c<'static, embassy_rp::peripherals::I2C1, embassy_rp::i2c::Async>;
