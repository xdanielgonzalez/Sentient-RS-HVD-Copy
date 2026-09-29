use super::digest::Digest;
use crate::error::Result;

/// A digest function. `finish` writes into a caller buffer sized for the digest (bounded, no alloc).
pub trait Hasher {
    /// The digest this hasher computes.
    fn digest(&self) -> Digest;
    /// Absorb input.
    fn update(&mut self, data: &[u8]);
    /// Write the digest into `out`; errors if `out` is smaller than the digest length.
    fn finish(&mut self, out: &mut [u8]) -> Result<usize>;
}
