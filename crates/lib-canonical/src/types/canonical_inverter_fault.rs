use serde::{Deserialize, Serialize};

/// Vendor-neutral inverter fault/warning flags decoded from a QPIWS-style status word. Novel
/// canonical type (no MIL-STD-3071 analogue); PROVISIONAL. `raw_bits` preserves the full QPIWS bit
/// field (bits 0..35, MSB-first, bit0 = string index 0) so an unmapped flag is never lost.
///
/// # Example
///
/// ```
/// use lib_canonical::CanonicalInverterFault;
///
/// // The parser decodes the named flags AND preserves the whole bit field.
/// let fault = CanonicalInverterFault {
///     line_fail: true,
///     raw_bits: (1 << 9) | (1 << 35),
///     ..Default::default()
/// };
/// assert!(fault.line_fail);
/// // Bits without a named flag stay queryable downstream — nothing is dropped.
/// assert_ne!(fault.raw_bits & (1 << 35), 0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CanonicalInverterFault {
    pub line_fail: bool,
    pub overtemperature_fault: bool,
    pub fan_locked_fault: bool,
    pub battery_low_fault: bool,
    pub overload_fault: bool,
    pub output_short_fault: bool,
    /// Raw QPIWS bit field (bits 0..35, MSB-first, bit0 = string index 0).
    pub raw_bits: u64,
}
