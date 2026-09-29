/// Where the TLS record AEAD keys actually live — the honesty accessor for the Secure/Non-Secure
/// split ADR 0013 §2 decides.
///
/// The decision is that traffic keys never cross the veneer as bytes: the Secure world does the
/// record crypto and Non-Secure holds only ciphertext, plaintext and opaque
/// [`KeyHandle`](super::key_handle::KeyHandle)s. The alternative — exporting derived traffic keys
/// to Non-Secure for speed — is a real, compile-time-gated escape hatch, and this enum is what
/// makes it impossible to take it silently.
///
/// Same doctrine as [`KeyCustody`](crate::profile::KeyCustody): a build that takes the weaker path
/// MUST report the weaker value, so no log, claim or attestation can mistake it for the default.
///
/// See ADR `docs/decisions/0013-transport-security-trust-store.md` §2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordProtection {
    /// Record encryption executes inside the Secure world (or another hardware isolation domain).
    /// Traffic keys never exist as bytes outside that boundary. This is the default and the only
    /// mode compatible with enclosing record encryption in a FIPS cryptographic boundary.
    SecureWorld,
    /// Derived traffic keys have been exported to the Non-Secure world, which performs the record
    /// AEAD itself. **Forbidden under `feature = "fips"`** — a FIPS boundary that covers the
    /// handshake but not the records is not a claim worth making. Reporting this value is how a
    /// build admits it took that path.
    NonSecureExported,
}
