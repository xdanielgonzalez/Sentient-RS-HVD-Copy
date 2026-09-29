use super::prelude::*;

impl Verifier for SoftwareVerifier {
    fn verify(
        &self,
        algorithm: Algorithm,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<()> {
        if algorithm != Algorithm::EcdsaP256Sha256 {
            return Err(SecurityError::NotImplemented);
        }
        verify_ecdsa_p256(public_key, message, signature)
    }
}
