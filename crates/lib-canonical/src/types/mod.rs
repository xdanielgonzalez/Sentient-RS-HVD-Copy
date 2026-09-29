//! Canonical device-model type definitions. One primary type per file (CLAUDE.md "Code
//! Organization"); behavior lives in the sibling `impl_*.rs` files at the crate root.

mod canonical_fault;
mod canonical_inverter_fault;
mod canonical_inverter_reading;
mod canonical_reading;
mod device_rail;
mod device_rail_reading;
mod inverter_mode;
mod inverter_ratings;

pub use canonical_fault::*;
pub use canonical_inverter_fault::*;
pub use canonical_inverter_reading::*;
pub use canonical_reading::*;
pub use device_rail::*;
pub use device_rail_reading::*;
pub use inverter_mode::*;
pub use inverter_ratings::*;
