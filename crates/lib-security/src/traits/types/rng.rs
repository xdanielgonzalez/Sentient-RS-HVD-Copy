use crate::error::Result;

/// A random byte source. Under FIPS this is a health-tested, approved DRBG; a health failure is an
/// [`crate::SecurityError::EntropyFailure`], never silent weak output.
pub trait Rng {
    /// Fill `out` with cryptographically secure random bytes, or fail closed.
    fn fill(&mut self, out: &mut [u8]) -> Result<()>;
}
