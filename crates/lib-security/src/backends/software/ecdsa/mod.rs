//! ECDSA P-256 verification primitive shared by [`super::keystore`] (via [`SoftwareVerifier`]) and
//! [`super::token`] (via [`verify_ecdsa_p256`]) — one delegation point so a token's signature and a
//! `KeyStore`-signed message are checked identically.

mod prelude;
mod types;
pub use types::*;

mod impl_software_verifier;

use prelude::*;

/// Raw ECDSA P-256 signature length (`r ‖ s`, 32 bytes each).
pub const SIGNATURE_LEN: usize = 64;

/// Raw SEC1 UNCOMPRESSED public-key point length (`0x04 ‖ x ‖ y`).
pub const PUBLIC_KEY_LEN: usize = 65;

/// The single delegation point: verify `signature` over `message` for `public_key` (raw SEC1
/// bytes). A parse failure of either key or signature is [`SecurityError::Malformed`]; a
/// non-verifying signature is [`SecurityError::VerificationFailed`] — the ONLY `Ok` is a
/// proven-good signature.
pub(super) fn verify_ecdsa_p256(public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<()> {
    let key = VerifyingKey::from_sec1_bytes(public_key).map_err(|_| SecurityError::Malformed)?;
    let sig = Signature::from_slice(signature).map_err(|_| SecurityError::Malformed)?;
    key.verify(message, &sig)
        .map_err(|_| SecurityError::VerificationFailed)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsupported_algorithm() {
        assert_eq!(
            SoftwareVerifier.verify(
                Algorithm::Ed25519,
                &[0u8; PUBLIC_KEY_LEN],
                b"m",
                &[0u8; SIGNATURE_LEN]
            ),
            Err(SecurityError::NotImplemented)
        );
    }

    #[test]
    fn malformed_public_key_is_rejected() {
        assert_eq!(
            verify_ecdsa_p256(&[0u8; 3], b"m", &[0u8; SIGNATURE_LEN]),
            Err(SecurityError::Malformed)
        );
    }

    #[test]
    fn malformed_signature_is_rejected() {
        // A REAL public key (all-zero bytes are not a valid SEC1 point and would fail at the KEY
        // parse step, not isolating the signature-parse failure this test targets).
        use super::super::keystore::SoftwareKeyStore;
        use super::super::rng::InsecureTestRng;
        use crate::traits::KeyStore;

        let mut store = SoftwareKeyStore::new(InsecureTestRng::new(1));
        let handle = store.generate(Algorithm::EcdsaP256Sha256).unwrap();
        let mut public_key = [0u8; PUBLIC_KEY_LEN];
        store.export_public(handle, &mut public_key).unwrap();

        assert_eq!(
            verify_ecdsa_p256(&public_key, b"m", &[0u8; 3]),
            Err(SecurityError::Malformed)
        );
    }
}
