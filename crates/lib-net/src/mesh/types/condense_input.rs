use lib_parse::canonical::{CanonicalFault, CanonicalReading};

#[cfg(feature = "locate")]
use crate::locate::LocationFix;

/// Input bundle for [`crate::mesh::condense`] — a named-field struct instead of a growing
/// positional-argument list (the 5th-argument shape the #387/#267 gap-analysis finding warned
/// against).
///
/// `fault` and `loc` are both optional; the greedy packer in
/// [`crate::mesh::impl_condense::write_fields`] emits whichever of `reading`'s fields, `fault`,
/// and `loc` fit before the caller's byte budget, in a fixed priority order — see that function's
/// doc comment for the exact order and the rationale for placing `fault` above `loc`.
#[derive(Debug, Clone, Copy)]
pub struct CondenseInput<'a> {
    /// The canonical BATTERY reading to condense — mandatory; this is what makes a frame worth
    /// sending at all.
    pub reading: &'a CanonicalReading,
    /// An active pack fault/warning set, if any (#387 — mesh fault forwarding). Encoded as a
    /// compact bitmask (see [`crate::mesh::impl_condense::fault_bitmask`]), not sixteen JSON keys.
    pub fault: Option<&'a CanonicalFault>,
    /// A location fix to tag the frame with, if one is available. Only present when this crate's
    /// `locate` feature is enabled (std/tokio-only): the MCU's bare `mesh` build has no location
    /// service and never threads one through (see WP3 — `McuPlatform::emit_reading`/
    /// `emit_fault` condense reading/fault only, no location).
    #[cfg(feature = "locate")]
    pub loc: Option<&'a LocationFix>,
}
