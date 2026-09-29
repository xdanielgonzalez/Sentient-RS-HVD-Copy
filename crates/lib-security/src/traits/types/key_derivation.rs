use super::key_handle::KeyHandle;
use crate::error::Result;

/// **The TLS 1.3 key schedule, kept inside the boundary.**
///
/// HKDF-Extract / HKDF-Expand-Label expressed handle-to-handle: a secret goes in as a
/// [`KeyHandle`], a derived secret comes out as a [`KeyHandle`], and no stage of the schedule
/// materializes as bytes on the caller's side. That is what lets the entire schedule — early
/// secret, handshake secret, master secret, traffic secrets — execute in the Secure world while
/// Non-Secure drives it, and what keeps a FIPS cryptographic boundary around the derivation rather
/// than only around the handshake signature.
///
/// # What a real implementation MUST do
///
/// * **Never export a secret through the handle-returning methods.** Across the Secure/Non-Secure
///   boundary, **derived secrets must never leave the Secure world**; only handles cross.
/// * **Gate [`expand_to_bytes`](KeyDerivation::expand_to_bytes) narrowly.** It exists only for
///   outputs that are legitimately public — an exporter value a peer also computes, a public
///   binder. **Finished keys, traffic secrets and traffic keys are NOT public**, and an
///   implementation that lets them out through this method has defeated the boundary. A
///   secure-world implementation SHOULD refuse labels outside an explicit allow-list with
///   [`crate::SecurityError::KeyExportForbidden`].
/// * **Fail closed on unknown handles and exhaustion.** An unknown or consumed handle is
///   [`crate::SecurityError::KeyNotFound`]; a full secret arena is an error, never a recycled slot.
/// * **Bind the hash.** The schedule's hash is fixed by the negotiated cipher suite; an
///   implementation must not silently substitute a different one.
///
/// See ADR `docs/decisions/0013-transport-security-trust-store.md` §1 and §2.
pub trait KeyDerivation {
    /// HKDF-Extract. `salt` and `ikm` are handles (a `None` means "the all-zero string of hash
    /// length", as RFC 8446's key schedule uses). Returns a handle to the pseudo-random key.
    fn extract(&mut self, salt: Option<KeyHandle>, ikm: Option<KeyHandle>) -> Result<KeyHandle>;

    /// HKDF-Expand-Label (RFC 8446 §7.1) producing `len` bytes of **secret** output, returned as a
    /// handle. This is the method every traffic secret and traffic key derivation must use.
    fn expand_label(
        &mut self,
        secret: KeyHandle,
        label: &[u8],
        context: &[u8],
        len: usize,
    ) -> Result<KeyHandle>;

    /// HKDF-Expand-Label writing the output into `out` as bytes, returning its length. **Only for
    /// outputs that are legitimately public** — Finished-key transcripts are NOT. See the trait doc
    /// for the allow-list obligation this method carries.
    fn expand_to_bytes(
        &mut self,
        secret: KeyHandle,
        label: &[u8],
        context: &[u8],
        out: &mut [u8],
    ) -> Result<usize>;
}
