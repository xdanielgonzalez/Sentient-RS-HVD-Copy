use lib_security::{
    Hasher, KeyAgreement, KeyDerivation, KeyStore, RecordCipher, Rng, TrustStore, Verifier,
};

/// The one type parameter every type in this crate takes: an aggregation over the `lib-security`
/// seam, so a TLS type needs **one** generic instead of eight.
///
/// ADR 0013 §1 is explicit that this is an *alias*, not a god-trait. It declares **no methods of
/// its own** and it is satisfied by a blanket impl (see `impl_secure_provider.rs`), so a backend
/// implements the eight underlying traits and gets this for free — there is nothing extra to
/// implement, nothing to keep in sync, and no second place where a cryptographic operation can be
/// declared. The alternative the ADR rejected was a `CryptoOps` trait that re-declared sign / verify
/// / random / digest; that would have duplicated the seam and given a backend author two places to
/// get the same operation wrong.
///
/// The bound list is exactly ADR 0013 §1's:
///
/// * [`TrustStore`] — peer trust **and** device identity, the single source of both. This is the
///   "one trust store, not two crypto stacks" requirement in trait form: the same handle serves the
///   TLS server identity and the DDS/TMS `tms.Identity` binding.
/// * [`KeyStore`] — signing with the device identity key, by opaque handle. There is deliberately
///   no `export_private` anywhere in the seam.
/// * [`KeyAgreement`] — ECDHE whose private half never leaves the Secure world; `agree` returns a
///   *handle* to the shared secret, not the secret.
/// * [`KeyDerivation`] — the TLS 1.3 key schedule, kept inside the boundary.
/// * [`RecordCipher`] — the record AEAD. Separate from `SessionCipher` on purpose: TLS 1.3's nonce
///   is `static_iv XOR seq` and is not transmitted, which `SessionCipher`'s counter-in-the-store
///   contract does not model.
/// * [`Verifier`], [`Hasher`], [`Rng`] — signature verification, transcript digests, and the
///   handshake's random material.
///
/// # Why this is generic rather than a concrete type
///
/// Non-Secure code links traits and one CMSE import library; it names no chip. An STM32H5 Secure
/// image that satisfies these same traits over its PKA/AES/RNG changes **zero** Non-Secure lines
/// (ADR 0013 §1, "Chip-agnosticism, concretely"). That property is the entire reason this crate is
/// being skeletoned before the chip decision lands rather than after it.
pub trait SecureProvider:
    TrustStore + KeyStore + KeyAgreement + KeyDerivation + RecordCipher + Verifier + Hasher + Rng
{
}
