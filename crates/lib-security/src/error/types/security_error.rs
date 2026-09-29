/// A security operation failure. `#[non_exhaustive]`: new failure modes must not silently map to
/// success at a call site.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecurityError {
    /// The selected provider does not implement this operation yet. Fail closed — never treat as
    /// success. Concrete backends are wired per `docs/security-integration-map.md`.
    NotImplemented,
    /// The requested algorithm is not in the approved set for the active tier (e.g. a non-approved
    /// primitive under `feature = "fips"`). The operation is refused, not downgraded.
    NotApproved,
    /// A power-on self-test / Known-Answer-Test failed; the crypto module must not be used.
    SelfTestFailed,
    /// The DRBG / entropy source failed a health test or is unavailable.
    EntropyFailure,
    /// A referenced key handle does not exist in the key store.
    KeyNotFound,
    /// An operation attempted to export or expose private key material across a boundary that
    /// forbids it (secure-world custody, or the no-export invariant). Always refused.
    KeyExportForbidden,
    /// A signature, MAC, or token signature did not verify.
    VerificationFailed,
    /// A credential (token, certificate) is outside its validity window.
    Expired,
    /// A certificate chain did not build to a trusted root.
    UntrustedChain,
    /// The presented identity (hostname / SAN / audience / issuer) did not match what was required.
    IdentityMismatch,
    /// An output buffer was too small for the result (bounded, no allocation).
    BufferTooSmall,
    /// Malformed input (bad encoding, wrong length, unparseable structure).
    Malformed,
    /// The underlying backend (software lib, secure element, TPM, PKCS#11) reported a failure.
    Backend,
}
