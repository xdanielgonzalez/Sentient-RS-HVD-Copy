use super::key_handle::KeyHandle;
use super::record_protection::RecordProtection;
use crate::error::Result;
use crate::profile::KeyCustody;

/// **TLS record AEAD — a separate trait from [`SessionCipher`](super::session_cipher::SessionCipher),
/// on purpose.**
///
/// # Why not widen `SessionCipher`
///
/// [`SessionCipher`](super::session_cipher::SessionCipher)'s contract is that the nonce is a per-key
/// monotonic counter *inside* the store, never caller-supplied, and that the nonce is carried on the
/// wire as a 12-byte prefix. TLS 1.3 needs neither: the record nonce is `static_iv XOR seq` and is
/// **not transmitted** — the sequence number is implicit, reconstructed by both ends. Widening
/// `SessionCipher` to accept a caller-supplied sequence number would weaken the nonce contract for
/// the existing payload path too, for the benefit of a different protocol. Two contracts, two
/// traits.
///
/// The safety property the payload path gets from an internal counter is preserved here by a
/// different mechanism: the **store** enforces strict per-(key, direction) monotonicity of `seq`.
/// The caller supplies the sequence number — as TLS requires — but **cannot cause nonce reuse**,
/// because a repeat or a regression is refused.
///
/// # What a real implementation MUST do
///
/// * **Enforce sequence monotonicity, per key and per direction.** A `seq` that repeats or goes
///   backwards fails closed with [`crate::SecurityError::VerificationFailed`]. This is the property
///   that makes a caller-supplied nonce input safe; an implementation without it is a nonce-reuse
///   bug, which for AES-GCM destroys both confidentiality and integrity for every record under that
///   key.
/// * **Refuse sequence-number exhaustion.** TLS 1.3 requires a key update before the 64-bit counter
///   wraps; wrapping must be an error, never a silent wrap-around.
/// * **Keep the traffic keys in the custody domain.** Across the Secure/Non-Secure boundary,
///   **traffic keys must never leave the Secure world**: the record buffer crosses the veneer,
///   the key does not. [`protection`](RecordCipher::protection) is how a build that takes the
///   key-export escape hatch admits it.
/// * **Leave no usable plaintext behind a failed open.** On authentication failure `buf` must hold
///   nothing a caller who ignored the `Result` could mistake for verified plaintext.
/// * **Write nothing on a too-small buffer.** [`crate::SecurityError::BufferTooSmall`], never a
///   truncated record.
///
/// # Framing
///
/// In place, no nonce prefix: the plaintext occupies `buf[..len]` on entry to
/// [`seal_record`](RecordCipher::seal_record) and the authentication tag is appended, so the return
/// is `len + TAG_LEN`. [`open_record`](RecordCipher::open_record) reverses it. `aad` is the TLS
/// record header.
///
/// See ADR `docs/decisions/0013-transport-security-trust-store.md` §1 and §2.
pub trait RecordCipher {
    /// Which custody tier the traffic keys actually have. **Mandatory honesty accessor** — same
    /// doctrine as [`KeyStore::custody`](super::key_store::KeyStore::custody).
    fn custody(&self) -> KeyCustody;

    /// Honesty accessor for the Secure/Non-Secure split: [`RecordProtection::SecureWorld`] or
    /// [`RecordProtection::NonSecureExported`]. A build that exports traffic keys to Non-Secure
    /// MUST report the latter — it is compile-time-gated and forbidden under `feature = "fips"`.
    fn protection(&self) -> RecordProtection;

    /// Seal `buf[..len]` in place under `key`, appending the tag; returns `len + TAG_LEN`. No nonce
    /// prefix — the nonce is `static_iv XOR seq` and is not transmitted.
    ///
    /// The store enforces strict per-(key, direction) monotonicity of `seq` and fails closed with
    /// [`crate::SecurityError::VerificationFailed`] on a repeat or regression — the caller supplies
    /// the sequence number but CANNOT cause nonce reuse.
    fn seal_record(
        &mut self,
        key: KeyHandle,
        seq: u64,
        aad: &[u8],
        buf: &mut [u8],
        len: usize,
    ) -> Result<usize>;

    /// Reverse [`seal_record`](RecordCipher::seal_record): authenticate and decrypt `buf[..len]` in
    /// place under `key`, `seq` and `aad`, returning the plaintext length. A tampered record is
    /// [`crate::SecurityError::VerificationFailed`], and on that failure `buf` holds no usable
    /// plaintext.
    fn open_record(
        &mut self,
        key: KeyHandle,
        seq: u64,
        aad: &[u8],
        buf: &mut [u8],
        len: usize,
    ) -> Result<usize>;
}
