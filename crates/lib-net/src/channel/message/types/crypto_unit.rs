#[cfg(feature = "packed")]
use serde::{Deserialize, Serialize};

use super::crypto_op::CryptoOp;
use crate::channel::message::CRYPTO_MAX;

/// **Crypto-handoff channel.** A unit crossing the app ↔ crypto (wolfCrypt/TLS) boundary. Carries
/// data plus an *opaque* session/key context id — **never key material** (keys stay behind the
/// `lib-security` KeyStore boundary). This keeps the crypto stage a separable, auditable
/// process: it is the only component that touches key handles.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "packed", derive(Serialize, Deserialize))]
pub struct CryptoUnit {
    /// Seal (plaintext → ciphertext) or open (ciphertext → plaintext).
    pub op: CryptoOp,
    /// Opaque session/key context id (a handle, not key bytes) selecting the keying material.
    pub context: u32,
    /// The data to seal or open, bounded.
    pub data: heapless::Vec<u8, CRYPTO_MAX>,
}
