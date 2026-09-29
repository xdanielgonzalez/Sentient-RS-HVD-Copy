use heapless::Vec as HVec;

use crate::traits::Rng;

use super::session_slot::SessionSlot;

/// Max session keys a [`SoftwareSessionStore`] holds at once (bounded, no allocation) — mirrors
/// [`super::super::super::keystore::ARENA_CAP`]'s idiom for the asymmetric key store.
pub const SESSION_ARENA_CAP: usize = 8;

/// Wire-framing nonce length in bytes. See [`crate::SessionCipher`]'s doc for the full
/// `[nonce][ciphertext][tag]` layout.
pub const NONCE_LEN: usize = 12;

/// Wire-framing authentication-tag length in bytes. See [`crate::SessionCipher`]'s doc.
pub const TAG_LEN: usize = 16;

/// [`crate::SessionCipher`] over a bounded in-memory arena of AES-256-GCM session keys (RustCrypto
/// `aes-gcm`) — the SOFTWARE tier's concrete session-cipher custody, mirroring
/// [`super::super::keystore::SoftwareKeyStore`]'s bounded-arena idiom.
///
/// `Debug` never leaks key material: `aes_gcm::Aes256Gcm` itself does not implement `Debug` (unlike
/// `ecdsa::SigningKey`, which does via its own `finish_non_exhaustive()`), so this store's `Debug`
/// impl (`impl_software_session_store.rs`) is hand-written and prints no fields.
pub struct SoftwareSessionStore<R: Rng> {
    pub(in crate::backends::software::session) rng: R,
    pub(in crate::backends::software::session) keys: HVec<SessionSlot, SESSION_ARENA_CAP>,
}
