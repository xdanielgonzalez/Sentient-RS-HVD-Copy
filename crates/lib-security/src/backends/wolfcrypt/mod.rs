//! wolfCrypt concrete backend (`feature = "wolfcrypt"`).
//!
//! Implements the [`crate::traits`] seam by delegating to wolfCrypt through its **RustCrypto trait
//! surface** — `digest::Digest` and `signature::Verifier`. Binding to the *traits* (not
//! wolfCrypt-internal types) keeps this delegation logic correct even when a concrete type PATH
//! needs a one-line fix on the build host: every `wolfssl_wolfcrypt::...` path below is collected in
//! the private [`wolf`] module and marked `// WOLFSSL-API: confirmed` with the source module read.
//!
//! ## Build ledger
//! `wolfssl-wolfcrypt` links a **system** wolfSSL 5.9.1+ library (built from the
//! `High-Velocity-Decisions/wolfssl-fork`, NOT vendored) and, for the FIPS tier, a FIPS 140-3
//! validated wolfCrypt module. It therefore does not link in the default CI/dev environment — this
//! is a **separate build ledger** (like the Zephyr `west` target), exercised by its own
//! `.github/workflows/wolfcrypt.yml` job, NOT the main gate. This bring-up targets the **std,
//! non-FIPS tier only**.
//!
//! The concrete type paths in [`wolf`] were confirmed by reading the `wolfssl-wolfcrypt` 2.0.0 crate
//! source directly (`random.rs`, `sha.rs`/`sha_digest.rs`, `ecdsa.rs`, `ed25519.rs`,
//! `rsa_pkcs1v15.rs`) — not guessed, not taken from docs.rs (which fails to build the crate without
//! a linkable wolfSSL).
//!
//! ## RNG: native fallible API, not the `rand_core` adapter
//! `wolfssl_wolfcrypt::random::RNG` implements `rand_core::TryRng` with `type Error =
//! core::convert::Infallible` — but its `try_fill_bytes` body calls the underlying wolfCrypt draw and
//! `.expect()`s success (see the crate's own doc: "wolfSSL RNG failures cause a panic, which is
//! consistent with the infallible contract"). Routing [`WolfRng::fill`] through that adapter would
//! turn a real DRBG health failure into an **unrecoverable panic** instead of
//! [`SecurityError::EntropyFailure`], which breaks this crate's fail-closed invariant (never panic,
//! always return `Err` on failure). [`WolfRng`] therefore calls `RNG::generate_block` directly — the
//! native `Result<(), i32>`-returning wolfCrypt entry point — and maps any failure to
//! `EntropyFailure`. (`rand_core::TryRngCore`/`RngCore` are also DEPRECATED aliases for
//! `TryRng`/`Rng` in rand_core 0.10.1; neither is used here.)
//!
//! ## RSA PKCS#1 v1.5: concrete, not generic
//! `wolfssl_wolfcrypt::rsa_pkcs1v15::VerifyingKey<H: Hash, const N: usize>` is generic over the hash
//! marker and modulus size in bytes, and — unlike the ECDSA/Ed25519 keys — does **not** implement
//! `TryFrom<&[u8]>` (only `Signature<N>` does; the key is built via `from_components(n, e)` or
//! `from_public_der(der)`). [`Algorithm::RsaPkcs1Sha256`] is narrowed to the single concrete
//! instantiation `VerifyingKey<rsa_pkcs1v15::Sha256, 256>` (RSA-2048 + SHA-256) and parses the public
//! key as a DER-encoded key via `from_public_der`, rather than through the generic `parse_key`
//! helper used for the other algorithms. **Semantic narrowing:** any RSA key whose modulus is not
//! exactly 2048 bits is rejected as [`SecurityError::Malformed`], not merely unsupported — widening
//! to other RSA sizes needs another concrete `VerifyingKey<Sha256, N>` arm.
//!
//! ## Honesty / fail-closed invariants preserved
//! * [`WolfKeyStore::custody`] returns [`KeyCustody::SoftwareMemory`] — wolfCrypt software keys are
//!   NOT hardware-backed; the accessor never claims otherwise (the top false-security trap).
//! * [`WolfVerifier`] returns `Ok(())` ONLY on a proven-good signature; a bad signature is
//!   [`SecurityError::VerificationFailed`], a parse failure is [`SecurityError::Malformed`].
//! * Key material operations that are not yet bound return [`SecurityError::NotImplemented`] (fail
//!   closed) rather than faking success.
//!
//! Composition with the posture toggles is documented in [`super`]: `fips` selects wolfCrypt FIPS
//! mode (the approved-algorithm gate and KAT hook stay in [`crate::provider`]); `trustzone` is
//! orthogonal and wolfCrypt software keys remain software custody.

mod prelude;
mod types;

pub use types::*;

mod impl_wolf_hasher;
mod impl_wolf_key_store;
mod impl_wolf_rng;
mod impl_wolf_session_store;
mod impl_wolf_verifier;

// =============================================================================================
// Tests — run ONLY on a wolfSSL-equipped host (`--features wolfcrypt`); never in the default build.
// =============================================================================================
#[cfg(all(test, feature = "wolfcrypt"))]
use prelude::*;

#[cfg(all(test, feature = "wolfcrypt"))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn keystore_reports_software_custody() {
        // Anti-false-security invariant: wolfCrypt software keys must NEVER report hardware custody.
        let ks = WolfKeyStore::new();
        assert_eq!(ks.custody(), KeyCustody::SoftwareMemory);
    }

    #[test]
    fn keystore_ops_fail_closed() {
        let mut ks = WolfKeyStore::new();
        assert_eq!(
            ks.generate(Algorithm::EcdsaP256Sha256),
            Err(SecurityError::NotImplemented)
        );
        assert_eq!(
            ks.import_private(Algorithm::EcdsaP256Sha256, &[0u8; 32]),
            Err(SecurityError::NotImplemented)
        );
    }

    #[test]
    fn session_store_reports_software_custody() {
        // Anti-false-security invariant: wolfCrypt software keys must NEVER report hardware custody.
        let store = WolfSessionStore::new();
        assert_eq!(store.custody(), KeyCustody::SoftwareMemory);
    }

    #[test]
    fn session_store_ops_fail_closed() {
        let mut store = WolfSessionStore::new();
        assert_eq!(store.establish(), Err(SecurityError::NotImplemented));
        let mut buf = [0u8; 32];
        assert_eq!(
            store.seal(KeyHandle(0), b"", &mut buf, 16),
            Err(SecurityError::NotImplemented)
        );
        assert_eq!(
            store.open(KeyHandle(0), b"", &mut buf, 32),
            Err(SecurityError::NotImplemented)
        );
    }

    #[test]
    fn sha256_known_answer() {
        // NIST FIPS 180-4 KAT: SHA-256("abc").
        const EXPECTED: [u8; 32] = [
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad,
        ];
        let mut h = WolfHasher::new(Digest::Sha256);
        h.update(b"abc");
        let mut out = [0u8; 32];
        let n = h.finish(&mut out).unwrap();
        assert_eq!(n, 32);
        assert_eq!(out, EXPECTED);
    }

    #[test]
    fn hasher_reports_buffer_too_small() {
        let mut h = WolfHasher::new(Digest::Sha256);
        h.update(b"abc");
        let mut tiny = [0u8; 8];
        assert_eq!(h.finish(&mut tiny), Err(SecurityError::BufferTooSmall));
    }

    #[test]
    fn rng_smoke_fills_and_varies() {
        let mut rng = WolfRng::new().expect("wolfCrypt DRBG instantiates on a wolfSSL host");
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        rng.fill(&mut a).expect("DRBG fills");
        rng.fill(&mut b).expect("DRBG fills");
        // Two 256-bit draws colliding is cryptographically negligible — a collision means a broken
        // or unseeded generator.
        assert_ne!(a, b);
        assert_ne!(a, [0u8; 32]);
    }
}
