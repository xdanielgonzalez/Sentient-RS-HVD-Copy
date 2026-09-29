use super::prelude::*;

impl core::fmt::Display for SecurityError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            SecurityError::NotImplemented => "operation not implemented by the active provider",
            SecurityError::NotApproved => "algorithm not approved for the active security tier",
            SecurityError::SelfTestFailed => "cryptographic self-test (KAT) failed",
            SecurityError::EntropyFailure => "entropy source / DRBG health failure",
            SecurityError::KeyNotFound => "key handle not found",
            SecurityError::KeyExportForbidden => {
                "private key export forbidden across this boundary"
            }
            SecurityError::VerificationFailed => "signature/MAC/token verification failed",
            SecurityError::Expired => "credential outside its validity window",
            SecurityError::UntrustedChain => "certificate chain did not reach a trusted root",
            SecurityError::IdentityMismatch => {
                "presented identity did not match the required identity"
            }
            SecurityError::BufferTooSmall => "output buffer too small",
            SecurityError::Malformed => "malformed input",
            SecurityError::Backend => "cryptographic backend failure",
        };
        f.write_str(s)
    }
}
