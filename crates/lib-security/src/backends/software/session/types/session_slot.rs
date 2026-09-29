use aes_gcm::Aes256Gcm;

/// One arena slot: an AES-256-GCM cipher instance plus its per-key monotonic nonce counter. The
/// counter — never the caller — picks every nonce [`crate::SessionCipher::seal`] uses; see that
/// trait's nonce-reuse-hazard doc.
pub(in crate::backends::software::session) struct SessionSlot {
    pub(in crate::backends::software::session) cipher: Aes256Gcm,
    pub(in crate::backends::software::session) nonce_counter: u64,
}
