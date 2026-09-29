/// The raw I2C0 peripheral in async mode (SDA=GP4, SCL=GP5 — the bench's actual INA3221 wiring;
/// see `boot_i2c.rs`'s module doc for how this was verified against the earlier, wrong, I2C1/GP2/GP3
/// wiring) — directly owned by the INA3221 rail-monitor leg alone (`boot_i2c.rs::build_i2c0`
/// constructs it, `boot_i2c.rs::build_rail_monitor` moves it into the driver). A different physical
/// peripheral than the OLED FeatherWing's fixed I2C1 (see
/// [`super::rp2350_i2c_bus::Rp2350I2c1Bus`]), so there is nothing here for the two consumers to
/// share.
#[cfg(feature = "power")]
pub(crate) type Rp2350I2c0Bus =
    embassy_rp::i2c::I2c<'static, embassy_rp::peripherals::I2C0, embassy_rp::i2c::Async>;
