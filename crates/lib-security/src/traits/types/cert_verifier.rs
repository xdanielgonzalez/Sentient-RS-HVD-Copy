#[cfg(feature = "std")]
use crate::error::Result;

/// X.509 certificate-chain verification. Host-side surface (needs alloc for chain building).
#[cfg(feature = "std")]
pub trait CertVerifier {
    /// Verify that `leaf` chains to one of `roots` through `intermediates`, is within its validity
    /// window at `now_unix`, and presents `expected_identity` (DNS SAN / CN). Fail closed.
    fn verify_chain(
        &self,
        leaf: &[u8],
        intermediates: &[&[u8]],
        roots: &[&[u8]],
        expected_identity: &str,
        now_unix: u64,
    ) -> Result<()>;
}
