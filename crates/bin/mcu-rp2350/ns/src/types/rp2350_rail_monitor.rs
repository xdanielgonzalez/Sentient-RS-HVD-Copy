use super::rp2350_i2c0_bus::Rp2350I2c0Bus;

/// The board's concrete INA3221 rail-monitor instantiation — `bin_mcu_platform::Ina3221RailMonitor`
/// (the newtype wrapper that lets `bin-mcu-platform` implement `spec_app::RailMonitor` for the
/// driver — see that type's own doc comment) generic over this crate's [`Rp2350I2c0Bus`] (I2C0,
/// GP4/GP5) — singly owned, no shared-bus device wrapper. Constructed once in
/// `boot_i2c.rs::build_rail_monitor`, then owned entirely by `boot_i2c::rail_task` — nothing else
/// touches it.
///
/// **This measures the adapter board's OWN rail draw (diagnostics/UI only) — NOT the project's
/// power source**, which is the Seplos BMS pack (`boot_seplos.rs`). See ADR 0012 for the
/// corrected record.
#[cfg(feature = "power")]
pub(crate) type Rp2350RailMonitor = bin_mcu_platform::Ina3221RailMonitor<Rp2350I2c0Bus>;
