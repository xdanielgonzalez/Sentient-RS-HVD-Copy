//! The BMS + inverter vendor protocol abstractions.
//!
//! A BMS vendor parser turns raw response bytes (from an injected transport adapter) into the
//! vendor-neutral [`crate::canonical::CanonicalReading`]/[`crate::canonical::CanonicalFault`], and builds request frames; an inverter
//! vendor parser does the same into [`crate::canonical::CanonicalInverterReading`]/[`crate::canonical::CanonicalInverterFault`]. The
//! two device classes are polled over different serial legs (RS-485 vs RS-232) with unrelated
//! wire protocols, so [`InverterVendorProtocol`] is a sibling trait, NOT a generalization of
//! [`BmsVendorProtocol`] (which is also MCU-consumed and must not grow inverter-shaped surface).
//! Vendors are interchangeable behind cargo features; callers depend on these traits, never a
//! concrete vendor.

mod impl_parse_error;
mod prelude;
mod types;

pub use types::*;
