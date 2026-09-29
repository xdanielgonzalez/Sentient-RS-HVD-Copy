/// [`crate::Verifier`] backed by wolfCrypt, delegating through `signature::Verifier`.
///
/// Fail closed: `Ok(())` is returned ONLY when the signature verifies. Any error path — a parse
/// failure of the key or signature ([`crate::SecurityError::Malformed`]) or a non-verifying signature
/// ([`crate::SecurityError::VerificationFailed`]) — denies.
pub struct WolfVerifier;
