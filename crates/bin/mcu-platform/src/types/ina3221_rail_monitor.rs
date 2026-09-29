use lib_io::ina3221::Ina3221;

/// Newtype wrapper enabling `impl spec_app::RailMonitor for Ina3221<I2C>` **from this crate**.
///
/// Neither `RailMonitor` (defined in `spec-app`) nor `Ina3221` (defined in `lib-io`) is local to
/// `bin-mcu-platform`, so Rust's orphan rule forbids implementing the trait directly on the driver
/// type here (a foreign trait for a foreign type, regardless of the dependency-graph direction
/// that made implementing it in `lib-io` itself a cycle — see `lib_io::ina3221`'s own module doc).
/// Wrapping the driver in a local newtype satisfies the orphan rule at zero runtime cost: this
/// type IS local to this crate, even though its single field is not.
///
/// [`core::ops::Deref`]/[`core::ops::DerefMut`] (see `impl_ina3221_rail_monitor.rs`) forward every
/// other `Ina3221` method (`init`, `identify`, …) transparently, so a caller mostly just uses this
/// wrapper exactly like the bare driver — `read_all` alone resolves to the [`RailMonitor`] impl
/// (Rust's method-resolution order checks the receiver's own inherent/trait methods before ever
/// following `Deref`), which is the whole point: the board crate's poll task calls `read_all`
/// through the seam, not the driver's inherent method directly.
pub struct Ina3221RailMonitor<I2C>(pub Ina3221<I2C>);
