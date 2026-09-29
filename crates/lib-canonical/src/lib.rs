//! Vendor-neutral canonical device model — the decoded, TMS-agnostic shape every vendor parser
//! produces and the TMS bridge consumes.
//!
//! Every feature-gated vendor parser in `lib-parse` produces these types from its wire format; the
//! downstream mapper (`lib-mapping`) consumes only these (never a vendor's own types, never a TMS
//! type). Values are raw fixed-point device units — scaling to engineering units is the mapper's
//! concern. Storage is bounded (`heapless`), so the model is `no_std`/`no_alloc`-friendly and
//! postcard-serializable for internal transport/logging.
//!
//! This is the SIBLING of `lib-tms` (the generated MIL-STD-3071 standard model): neither crate
//! depends on the other. The `lib-mapping` bridge is the single place canonical → TMS happens.
//!
//! # Example
//!
//! Build a reading exactly as a vendor parser would — raw fixed-point device units in, no
//! scaling applied:
//!
//! ```
//! use heapless::Vec;
//! use lib_canonical::CanonicalReading;
//!
//! let reading = CanonicalReading {
//!     device_address: 0x01,
//!     // Per-cell millivolts, bounded by MAX_CELLS (heapless — no allocation).
//!     cell_voltages_mv: Vec::from_slice(&[3312, 3308, 3310, 3311])?,
//!     // Seplos reports temperatures as Kelvin×10: 2981 → 25.0 °C.
//!     temperatures_raw: Vec::from_slice(&[2981, 2978])?,
//!     current_ca: 250,                // +2.50 A — positive = charging (vendor convention)
//!     total_voltage_cv: 5312,         // 53.12 V
//!     remaining_capacity_cah: 18_500, // 185.00 Ah
//!     typical_capacity_cah: 23_000,
//!     rated_capacity_cah: 23_000,
//!     soc_dpct: 804,                  // 80.4 %
//!     soh_dpct: 1000,                 // 100.0 %
//!     cycles: 12,
//!     port_voltage_cv: 5309,
//! };
//!
//! // Values stay raw; the mapper scales at the edge (Kelvin×10 → °C shown here).
//! assert_eq!(reading.cell_voltages_mv.len(), 4);
//! let kelvin_x10 = i32::from(reading.temperatures_raw[0]);
//! assert_eq!((kelvin_x10 - 2731) / 10, 25);
//! # Ok::<(), heapless::CapacityError>(())
//! ```
#![no_std]
#![forbid(unsafe_code)]

mod impl_canonical_fault;
mod impl_canonical_inverter_reading;
mod types;

pub use types::*;

/// Maximum cells represented in one reading (bounds the heapless storage).
pub const MAX_CELLS: usize = 16;
/// Maximum temperature sensors in one reading.
pub const MAX_TEMPS: usize = 8;
