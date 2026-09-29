//! `no_std` JSON codec for the TMS schema, via [`serde-json-core`]. Serializes into a caller-provided
//! fixed buffer (no `alloc`) so the same codec runs on the RPi and the MCU forward path. Floats that
//! are `NaN` / `±Inf` are not valid JSON (the transform uses `NaN` for "not available"), so callers
//! that must stay strictly JSON-valid should populate only known fields.

use serde::Serialize;

/// Codec error from the JSON codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecError {
    /// The output buffer was too small for the serialized payload.
    BufferTooSmall,
}

impl core::fmt::Display for CodecError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CodecError::BufferTooSmall => write!(f, "output buffer too small"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CodecError {}

/// Serialize a TMS payload to JSON into `buf`, returning the number of bytes written.
///
/// `no_std`, no allocation. Note: floats that are `NaN`/`±Inf` are not valid JSON — the transform
/// uses `NaN` for "not available", so callers that must stay strictly JSON-valid should populate
/// only known fields (a NaN→null adapter is a follow-up).
///
/// # Examples
///
/// ```
/// use lib_mapping::codec::{to_json_slice, CodecError};
/// use lib_mapping::schema::ClockMonotonic;
///
/// let stamp = ClockMonotonic { seconds: 12, nanoseconds: 500_000_000 };
/// let mut buf = [0u8; 64];
/// let n = to_json_slice(&stamp, &mut buf)?;
/// assert_eq!(&buf[..n], br#"{"seconds":12,"nanoseconds":500000000}"#);
///
/// // An undersized buffer is reported, never overrun.
/// let mut tiny = [0u8; 8];
/// assert_eq!(to_json_slice(&stamp, &mut tiny), Err(CodecError::BufferTooSmall));
/// # Ok::<(), CodecError>(())
/// ```
pub fn to_json_slice<T: Serialize>(value: &T, buf: &mut [u8]) -> Result<usize, CodecError> {
    serde_json_core::to_slice(value, buf).map_err(|_| CodecError::BufferTooSmall)
}
