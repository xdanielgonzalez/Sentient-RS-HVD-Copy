use super::secure_provider::SecureProvider;

/// The rustls `CryptoProvider` routed through the `lib-security` seam — a skeleton (ADR 0013 §3,
/// WP-3).
///
/// # Why this type has to exist at all
///
/// rustls ships two providers and **neither one builds for this target**. `ring` explicitly declined
/// Thumb support (issue #813, closed as not planned); `aws-lc-rs` — rustls's only FIPS-capable
/// provider — is not a bare-metal target. A provider must therefore be hand-built.
///
/// That is the constraint, and it is also the design's best piece of luck: a hand-built provider is
/// a provider whose every primitive can be routed into the Secure world through
/// [`SecureProvider`](crate::SecureProvider). Signature verification is `Verifier`, transcript
/// digests are `Hasher`, handshake randomness is `Rng`, ECDHE is `KeyAgreement`, the TLS 1.3 key
/// schedule is `KeyDerivation`, and record AEAD is `RecordCipher`. Private keys and traffic keys
/// never cross the veneer as bytes — Non-Secure holds ciphertext, plaintext and opaque handles only
/// (ADR 0013 §2).
///
/// # The FIPS consequence, stated plainly
///
/// A rustls deployment is **non-FIPS by construction** — its only FIPS-capable provider does not
/// build here. What §2's boundary decision buys is that the FIPS question is answered by *what the
/// provider calls*, and this provider calls the seam. Swapping in a validated module behind
/// `RecordCipher` / `KeyStore` therefore changes a backend, not the architecture. That is
/// forward-compatibility, not a FIPS claim, and this crate must never be documented as more.
///
/// # What it is right now
///
/// A refusal. Every method returns `Err(TlsError::Unimplemented)` — including
/// [`fill_random`](crate::SeamCryptoProvider::fill_random), which is worth calling out: a random
/// source that silently returned a fixed or zero buffer would compromise every key derived from it
/// while looking perfectly healthy. It fails instead.
///
/// TODO(ADR 0013 WP-3): implement `rustls::crypto::CryptoProvider` over this type — the
/// `SupportedKxGroup` / `SupportedCipherSuite` tables (TLS 1.3 only, EC only), a `SecureRandom` over
/// `Rng`, and a `KeyProvider` that yields a signing key backed by `KeyStore::sign` on the device
/// identity handle rather than by any exported private key.
pub struct SeamCryptoProvider<P: SecureProvider> {
    /// The Secure-world seam every primitive routes through.
    pub(crate) provider: P,
}
