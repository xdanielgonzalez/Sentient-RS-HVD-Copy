use super::kex_group::KexGroup;
use super::key_handle::KeyHandle;
use crate::error::Result;
use crate::profile::KeyCustody;

/// **Ephemeral elliptic-curve Diffie-Hellman without exporting the private half.**
///
/// The TLS 1.3 (and, if it is ever required, DDS-Security PKI-DH) key exchange, expressed so that
/// neither the ephemeral private key nor the resulting shared secret ever crosses this trait as
/// bytes. Only the *public* share is copied out; [`agree`](KeyAgreement::agree) returns a
/// [`KeyHandle`] **to** the shared secret, which [`KeyDerivation`](super::key_derivation::KeyDerivation)
/// then consumes by handle. The secret is born inside the store and dies there — exactly the
/// doctrine [`KeyStore`](super::key_store::KeyStore) applies to signing keys and
/// [`SessionCipher`](super::session_cipher::SessionCipher) applies to session keys.
///
/// # What a real implementation MUST do
///
/// * **Keep the private half inside the custody domain.** There is deliberately no method that
///   returns the ephemeral private key or the shared secret. Across the Secure/Non-Secure boundary
///   **private key material and the derived shared secret must never leave the Secure world**;
///   handles cross the veneer, key bytes do not.
/// * **Validate the peer's share.** `peer_share` is attacker-controlled. An implementation MUST
///   reject a point that is not on the curve, is the point at infinity, or is not in the correct
///   subgroup, with [`crate::SecurityError::Malformed`] — an unvalidated point is an invalid-curve
///   attack that leaks the private scalar.
/// * **Erase the ephemeral on use.** [`agree`](KeyAgreement::agree) consumes `handle`: the
///   ephemeral private key is zeroized and the handle is invalid afterwards. Reusing a consumed
///   handle MUST fail with [`crate::SecurityError::KeyNotFound`], never silently re-derive.
/// * **Fail closed on exhaustion.** A full ephemeral arena returns an error, never a recycled or
///   fabricated handle.
///
/// See ADR `docs/decisions/0013-transport-security-trust-store.md` §1 and §2.
pub trait KeyAgreement {
    /// Which custody tier the ephemeral private keys and derived secrets actually have.
    /// **Mandatory honesty accessor** — same doctrine as
    /// [`KeyStore::custody`](super::key_store::KeyStore::custody).
    fn custody(&self) -> KeyCustody;

    /// Generate an ephemeral key pair in `group` inside the store; returns an opaque handle to the
    /// private half. The private key never leaves the store.
    fn generate_ephemeral(&mut self, group: KexGroup) -> Result<KeyHandle>;

    /// Copy out our public share (public data) — e.g. an uncompressed P-256 point — and return its
    /// length. An `out` too small fails closed with [`crate::SecurityError::BufferTooSmall`].
    fn public_share(&self, handle: KeyHandle, out: &mut [u8]) -> Result<usize>;

    /// Combine with the peer's share. Returns a **handle to the shared secret**, not the secret.
    /// Consumes `handle` (the ephemeral private key is erased), so `handle` is invalid on return
    /// whether this succeeded or failed.
    fn agree(&mut self, handle: KeyHandle, peer_share: &[u8]) -> Result<KeyHandle>;
}
