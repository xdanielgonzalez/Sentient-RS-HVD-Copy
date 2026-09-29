//! Vendor-neutral canonical battery + inverter models.
//!
//! Every feature-gated vendor parser produces these types from its wire format; the downstream
//! TMS mapper consumes only these (never a vendor's own types). Values are raw fixed-point device
//! units — scaling to engineering units is the mapper's concern. Storage is bounded (`heapless`),
//! so the model is `no_std`/`no_alloc`-friendly.
//!
//! The types themselves live in [`lib_canonical`] (postcard-serializable, the vendor-neutral
//! device model) and are re-exported here so existing `lib_parse::canonical::*` call sites —
//! including the Seplos decoder in [`crate::seplos`] — keep resolving without churn. The inverter
//! types (`CanonicalInverterReading`/`CanonicalInverterFault`/`InverterMode`/`InverterRatings`)
//! are novel, no MIL-STD-3071 analogue; consumed by the RS-232 Voltronic decoder in
//! [`crate::voltronic`] (feature `voltronic`).

pub use lib_canonical::{CanonicalFault, CanonicalReading, MAX_CELLS, MAX_TEMPS};
pub use lib_canonical::{
    CanonicalInverterFault, CanonicalInverterReading, InverterMode, InverterRatings,
};
