use super::algorithm::Algorithm;
use crate::error::Result;

/// Signature verification over raw public keys. Fail closed: `Ok(())` means the signature verified
/// under `algorithm` for `public_key` over `message`; any error means reject.
pub trait Verifier {
    /// Verify `signature` over `message` for `public_key` under `algorithm`.
    fn verify(
        &self,
        algorithm: Algorithm,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<()>;
}
