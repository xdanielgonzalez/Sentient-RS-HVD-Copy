/// Signature / key algorithm identifiers. The approved subset depends on the active tier; under
/// FIPS a non-approved variant yields [`crate::SecurityError::NotApproved`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Algorithm {
    /// Ed25519 EdDSA (not FIPS 140-3 approved — refused when `feature = "fips"`).
    Ed25519,
    /// ECDSA P-256 with SHA-256 (approved).
    EcdsaP256Sha256,
    /// ECDSA P-384 with SHA-384 (approved).
    EcdsaP384Sha384,
    /// RSA PKCS#1 v1.5 2048 with SHA-256 (approved).
    RsaPkcs1Sha256,
}
