//! [`CdrError`] — the error returned by the XCDR2 decode path ([`crate::cdr::decode`]).

/// Failure decoding an XCDR2 (DDS-XTypes 1.3) byte stream. Payload-free so it compiles in `no_std`
/// with no allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdrError {
    /// The buffer ended before a field that the type layout requires (out-of-bounds read).
    Truncated,
    /// The 2-byte RTPS encapsulation identifier was not one this decoder accepts. This decoder
    /// reads the little-endian XCDR2 encapsulations `CDR2_LE` (`{0x00,0x07}`) and `D_CDR2_LE`
    /// (`{0x00,0x09}`) — DDS-XTypes 1.3 §7.6.3.1.2 Table 60.
    BadEncapsulation,
    /// A DHEADER declared a subtree length that runs past the end of the buffer, or a length prefix
    /// (string/sequence) is inconsistent with the remaining bytes.
    BadLength,
    /// A `string` field's length prefix or NUL terminator violated the XCDR2 string rule
    /// (DDS-XTypes 1.3 §7.4.3.5 rule (3)), or its bytes were not valid UTF-8.
    BadString,
    /// An enumerated field held an integer with no corresponding variant for its type.
    BadEnum,
    /// A `boolean` (or optional `<is_present>` marker) byte was neither `0x00` nor `0x01`.
    BadBool,
    /// A bounded collection field exceeded the target type's capacity.
    CapacityExceeded,
}
