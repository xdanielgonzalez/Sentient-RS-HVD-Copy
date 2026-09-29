use super::key_handle::KeyHandle;
use crate::error::Result;
use crate::profile::KeyCustody;

/// An authenticated-encryption session over opaque, in-store keys — the consumer of the channel
/// layer's `CryptoUnit { op: Seal | Open, context: u32, data }` (see `lib-net`'s `CryptoStage`).
/// Exactly like [`super::key_store::KeyStore`], a session key is **born inside the store** and
/// never crosses this trait as bytes — [`establish`](SessionCipher::establish) returns an opaque
/// [`KeyHandle`], not key material.
///
/// ## Wire framing (deterministic, in-place)
/// [`seal`](SessionCipher::seal) and [`open`](SessionCipher::open) operate on a single caller-owned
/// `buf: &mut [u8]` — no allocation, no realloc, `no_std`-friendly. The **plaintext** occupies
/// `buf[..len]` on entry to `seal`; on success `seal` has overwritten `buf` with the framed
/// ciphertext and returns its length. The concrete backends in this crate use:
///
/// ```text
/// [ 12-byte nonce ][ ciphertext (== plaintext length) ][ 16-byte tag ]
/// ```
///
/// i.e. `sealed_len == len + 28`. `buf` must be at least that long or `seal` fails closed with
/// [`crate::SecurityError::BufferTooSmall`] — it never truncates or partially writes. `open`
/// reverses the layout: it reads the leading 12-byte nonce and trailing 16-byte tag out of
/// `buf[..len]`, authenticates the middle ciphertext span against `aad`, and on success overwrites
/// `buf[..plaintext_len]` with the recovered plaintext (`plaintext_len == len - 28`) and returns
/// that length. A `len` too short to hold the framing is [`crate::SecurityError::Malformed`] (it
/// describes a bad *input*, not an undersized *output* buffer).
///
/// A future non-AES-GCM backend that needs a different nonce/tag size is free to use a different
/// framing — the sizes above are this crate's current concrete backends' choice (AES-256-GCM: a
/// 96-bit nonce, a 128-bit tag), not an associated constant of the trait itself. Each backend
/// documents and exports its own `NONCE_LEN`/`TAG_LEN`.
///
/// ## Nonce-reuse hazard
/// Reusing a (key, nonce) pair for AES-GCM is catastrophic — it breaks both confidentiality and
/// integrity for every message sealed under that nonce. The nonce is therefore a **per-key
/// monotonic counter that lives INSIDE the store**, never a value the caller supplies or the trait
/// exposes: `seal` draws the next counter value, encodes it into the wire framing, and advances the
/// counter before returning. Key bytes and nonce state never cross this trait in either direction —
/// exactly like [`super::key_store::KeyStore`] never exports a private key, a `SessionCipher` never
/// exports a key OR lets a caller pick (and thus collide) a nonce.
pub trait SessionCipher {
    /// Which custody tier this store actually provides. **Mandatory honesty accessor** — same
    /// doctrine as [`super::key_store::KeyStore::custody`]: a software session store MUST report
    /// [`KeyCustody::SoftwareMemory`] so callers, logs, and customer-facing claims can never mistake
    /// it for hardware-bound custody.
    fn custody(&self) -> KeyCustody;

    /// Generate a new session key *inside* the store; returns an opaque handle. The key never
    /// leaves the store as bytes. Fails closed (never a fake handle) if the store's key arena is
    /// exhausted.
    fn establish(&mut self) -> Result<KeyHandle>;

    /// Seal `buf[..len]` (plaintext) in place under the key referenced by `handle`, authenticating
    /// `aad` alongside it. On success, `buf` holds `[nonce][ciphertext][tag]` and the return value
    /// is that framed length (`len` plus the framing overhead) — see the trait doc for the exact
    /// layout. An unknown `handle` is [`crate::SecurityError::KeyNotFound`]; a `buf` too small to
    /// hold the framed output is [`crate::SecurityError::BufferTooSmall`] (fail closed — never a
    /// partial or truncated write).
    fn seal(&mut self, handle: KeyHandle, aad: &[u8], buf: &mut [u8], len: usize) -> Result<usize>;

    /// Reverse [`seal`](SessionCipher::seal): authenticate and decrypt the framed `buf[..len]`
    /// under the key referenced by `handle` and `aad`, overwriting `buf` with the recovered
    /// plaintext and returning its length. An unknown `handle` is
    /// [`crate::SecurityError::KeyNotFound`]; a `len` too short to hold the framing is
    /// [`crate::SecurityError::Malformed`]; a tampered ciphertext or tag is
    /// [`crate::SecurityError::VerificationFailed`] — and on that failure `buf` is left holding NO
    /// usable plaintext (each backend documents whether it zeroes or otherwise scrubs the buffer),
    /// so a caller that forgets to check the `Result` can never observe unauthenticated plaintext.
    fn open(&mut self, handle: KeyHandle, aad: &[u8], buf: &mut [u8], len: usize) -> Result<usize>;
}
