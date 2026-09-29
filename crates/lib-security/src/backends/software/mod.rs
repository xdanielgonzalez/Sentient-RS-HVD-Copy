//! Software cryptographic backend (`feature = "software"`, ON by default).
//!
//! Pure-Rust ECDSA P-256 (RustCrypto `p256`/`ecdsa`), `no_std` + no-`alloc` (this module's `p256`
//! dependency is pulled in with `default-features = false, features = ["ecdsa"]` — no "std", no
//! "alloc", no "getrandom"). This is the concrete backend for the **SOFTWARE / non-FIPS tier**:
//! private keys live in [`keystore::SoftwareKeyStore`]'s own arena, in normal-world process memory,
//! zeroized on drop (`ecdsa::SigningKey`'s own `Drop` impl zeroizes the secret scalar — see the
//! RustCrypto source) — never hardware-bound. It is deliberately **not** a FIPS backend and never
//! claims to be one; `wolfcrypt` is the separate FIPS-*capable* path (system wolfSSL, its
//! own build ledger). [`KeyStore::custody`] always reports
//! `KeyCustody::SoftwareMemory` (`crate::KeyCustody`) — the crate's mandatory honesty accessor.
//!
//! ## Algorithm: ECDSA P-256 / SHA-256 only
//! [`Algorithm::EcdsaP256Sha256`] is the only algorithm this backend implements — it is
//! simultaneously FIPS-140-3-approved (so selecting it here still composes correctly if a caller
//! later checks [`crate::Provider::check_approved`]) and the curve rustls/TLS actually negotiates
//! (`SignatureScheme::ECDSA_NISTP256_SHA256` — see `lib-link::tls_identity::scheme_for`). Any
//! other requested [`Algorithm`] is refused with [`crate::SecurityError::NotImplemented`] (fail closed —
//! this backend truly does not implement it, as opposed to [`crate::SecurityError::NotApproved`], which is
//! the FIPS *policy* gate over algorithms a backend DOES implement).
//!
//! ## Module map (one concern per file)
//! * `rng` — the [`Rng`](crate::Rng) seam adapter to `rand_core`, and [`InsecureTestRng`] (a
//!   deterministic, non-cryptographic test/demo generator).
//! * `ecdsa` — the shared ECDSA P-256 verify primitive ([`SoftwareVerifier`]) both the key store
//!   and the token verifier delegate to, so a signature is checked identically everywhere.
//! * `keystore` — [`SoftwareKeyStore`]: the bounded, opaque-handle key arena (`KeyStore` impl).
//! * `session` — [`SoftwareSessionStore`]: the bounded, opaque-handle AES-256-GCM session-key
//!   arena (`SessionCipher` impl) the channel layer's `CryptoUnit`/`CryptoStage` consumes.
//! * `token` — [`SoftwareTokenVerifier`] + [`sign_token`]: the signed-claims token format.
//!
//! See each submodule's doc for its specific design (key custody / seeding, token wire format).

mod ecdsa;
mod keystore;
mod rng;
mod session;
mod token;

pub use ecdsa::{SoftwareVerifier, PUBLIC_KEY_LEN, SIGNATURE_LEN};
pub use keystore::{SoftwareKeyStore, ARENA_CAP};
pub use rng::InsecureTestRng;
pub use session::{SoftwareSessionStore, NONCE_LEN, SESSION_ARENA_CAP, TAG_LEN};
pub use token::{sign_token, SoftwareTokenVerifier, TOKEN_MAX};

use crate::error::Result;
use crate::traits::{Algorithm, KeyStore, Verifier};

/// An internal-consistency self-test for this backend: generate an ECDSA P-256 key (seeded by a
/// FIXED, non-secret [`InsecureTestRng`] seed — deterministic and reproducible, not a real key),
/// sign a fixed message, and verify the signature against the derived public key through
/// [`SoftwareVerifier`] — the same code path [`SoftwareKeyStore`]/[`SoftwareTokenVerifier`] use in
/// production.
///
/// This is NOT a NIST-published Known-Answer-Test (there is no external reference vector checked
/// against — deterministic RFC6979 signing makes the *output* reproducible, but this test does not
/// hardcode and assert the expected signature bytes); it is a round-trip health check that the
/// linked `p256`/`ecdsa` stack signs and verifies correctly. It is NOT wired into
/// `run_power_on_self_tests` (that hook is FIPS-only by contract — this backend
/// is a different, non-FIPS tier); callers of this backend may invoke it directly as a fail-closed
/// sanity check before trusting a `SoftwareKeyStore`/`SoftwareTokenVerifier` instance.
pub fn self_test() -> Result<()> {
    const FIXED_SEED: u64 = 0x5EC1_D5E1_ED5E_ED00;
    const MESSAGE: &[u8] = b"lib-security software backend self-test";

    let mut store = SoftwareKeyStore::new(InsecureTestRng::new(FIXED_SEED));
    let handle = store.generate(Algorithm::EcdsaP256Sha256)?;

    let mut public_key = [0u8; PUBLIC_KEY_LEN];
    let pk_len = store.export_public(handle, &mut public_key)?;

    let mut signature = [0u8; SIGNATURE_LEN];
    let sig_len = store.sign(handle, MESSAGE, &mut signature)?;

    SoftwareVerifier.verify(
        Algorithm::EcdsaP256Sha256,
        &public_key[..pk_len],
        MESSAGE,
        &signature[..sig_len],
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    #[test]
    fn backend_self_test_passes() {
        super::self_test().unwrap();
    }
}
