/// Concrete wolfCrypt type paths, isolated so a build-host engineer confirms them in ONE place.
///
/// Every alias here is CONFIRMED against the `wolfssl-wolfcrypt` 2.0.0 crate source (read directly —
/// `random.rs`, `sha.rs`, `sha_digest.rs`, `ecdsa.rs`, `ed25519.rs`, `rsa_pkcs1v15.rs`). Bind to the
/// RustCrypto trait bounds (see the `where` clauses at each use site) so a future upstream rename
/// only needs a PATH fix here, not a delegation-logic rewrite.
pub(crate) mod wolf {
    // WOLFSSL-API: confirmed — wolfssl-wolfcrypt 2.0.0 `src/random.rs`.
    // The wolfCrypt CSPRNG context (`WC_RNG`). `RNG::new()` wraps `wc_InitRng`;
    // `RNG::generate_block` wraps `wc_RNG_GenerateBlock`. Both are the crate's native, fallible
    // (`Result<_, i32>`) entry points — see the module doc comment above for why we bind to these
    // directly rather than the `rand_core::TryRng` impl.
    pub use wolfssl_wolfcrypt::random::RNG as Rng;

    // WOLFSSL-API: confirmed — wolfssl-wolfcrypt 2.0.0 `src/sha.rs` (types) + `src/sha_digest.rs`
    // (the `digest::Digest`-family trait impls, gated on `feature = "digest"`). NOT `sha256`/
    // `sha384`/`sha512` submodules — SHA-2 lives directly in the `sha` module.
    pub use wolfssl_wolfcrypt::sha::{SHA256 as Sha256, SHA384 as Sha384, SHA512 as Sha512};

    // WOLFSSL-API: confirmed — wolfssl-wolfcrypt 2.0.0 `src/ed25519.rs` (`signature_impl` submodule,
    // re-exported at `ed25519::{Signature, VerifyingKey}`, gated on `feature = "signature"`).
    // `impl signature::Verifier<Signature> for VerifyingKey` is present.
    pub use wolfssl_wolfcrypt::ed25519::{Signature as Ed25519Sig, VerifyingKey as Ed25519Key};

    // WOLFSSL-API: confirmed — wolfssl-wolfcrypt 2.0.0 `src/ecdsa.rs`. Per-curve types are at the
    // module root (macro-generated), NOT nested under `ecdsa::p256`/`ecdsa::p384`.
    pub use wolfssl_wolfcrypt::ecdsa::{
        P256Signature as P256Sig, P256VerifyingKey as P256Key, P384Signature as P384Sig,
        P384VerifyingKey as P384Key,
    };

    // WOLFSSL-API: confirmed — wolfssl-wolfcrypt 2.0.0 `src/rsa_pkcs1v15.rs`. `VerifyingKey<H: Hash,
    // const N: usize>` and `Signature<const N: usize>` are GENERIC (not `rsa::pkcs1v15::...`); `H`
    // is a per-hash marker enum in this same module (`Sha256`/`Sha384`/`Sha512`), `N` is the modulus
    // size in bytes. `Algorithm::RsaPkcs1Sha256` is narrowed to the concrete RSA-2048 instantiation —
    // see the module doc comment above.
    pub use wolfssl_wolfcrypt::rsa_pkcs1v15::{
        Sha256 as RsaHashSha256, Signature as RsaSignature, VerifyingKey as RsaVerifyingKey,
    };

    /// RSA-2048 / SHA-256 PKCS#1 v1.5 verifying key — the sole concrete instantiation backing
    /// [`Algorithm::RsaPkcs1Sha256`](crate::traits::Algorithm::RsaPkcs1Sha256). `256` is the RSA-2048
    /// modulus size in bytes.
    pub type RsaKey = RsaVerifyingKey<RsaHashSha256, 256>;
    /// Fixed-size signature matching [`RsaKey`] (256 bytes for RSA-2048).
    pub type RsaSig = RsaSignature<256>;
}
