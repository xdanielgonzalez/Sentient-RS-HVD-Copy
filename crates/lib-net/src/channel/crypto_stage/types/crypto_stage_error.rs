use lib_security::SecurityError;

/// [`super::CryptoStage`]'s typed fault surface. Fail-closed: either variant means the
/// [`crate::channel::stage::Stage::process`] call returns `Err`, so
/// [`crate::channel::stage::drive`] drops the item instead of forwarding a partially sealed/opened
/// (untrusted) [`crate::channel::message::CryptoUnit`].
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoStageError {
    /// The underlying `SessionCipher::seal`/`open` call failed — key not found, verification
    /// failed (tampered ciphertext), backend failure, etc. See the wrapped [`SecurityError`] for
    /// which.
    Cipher(SecurityError),
    /// Sealing needed more room than the boundary message's fixed capacity
    /// ([`crate::channel::message::CRYPTO_MAX`]) allows — the plaintext plus the cipher's
    /// nonce/tag framing would overflow the bound. Distinct from a `SecurityError::BufferTooSmall`
    /// reported *by* the cipher (that means "the cipher itself refused the buffer it was given");
    /// this means "the `CryptoUnit` has no headroom left to give it" — both fail closed the same
    /// way, but the caller can tell which layer said no.
    CapacityExceeded,
}
