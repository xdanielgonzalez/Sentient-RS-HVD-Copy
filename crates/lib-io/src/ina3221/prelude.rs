//! Shared imports for the `ina3221` module's `impl_*` files.

pub use super::types::*;
pub use super::{
    decode_bus_mv, decode_shunt_uv, CONFIG_RESET, I2C_ADDR_DEFAULT, REG_CONFIG, REG_DIE_ID,
};
// `pub(crate)`, not `pub`: `BUS_REGS`/`SHUNT_REGS` are themselves `pub(crate)` in `super` (an
// internal register-index lookup, not part of this crate's public API), so re-exporting them any
// wider than `pub(crate)` here would fail to compile (E0364) — this module (`prelude`) is itself
// private, so the restriction costs nothing.
pub(crate) use super::{BUS_REGS, SHUNT_REGS};
pub use embedded_hal_async::i2c::I2c;
pub use lib_canonical::{DeviceRail, DeviceRailReading};
