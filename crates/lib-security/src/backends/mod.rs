//! Concrete cryptographic backends behind the provider seam.
//!
//! Each backend is its own feature. A backend supplies concrete implementations of the
//! [`crate::traits`] contracts ([`Rng`](crate::Rng), [`Hasher`](crate::Hasher),
//! [`Verifier`](crate::Verifier), [`KeyStore`](crate::KeyStore), [`TokenVerifier`](crate::TokenVerifier)).
//! With every backend feature off, the seam's fail-closed stubs ([`crate::provider`]) remain the
//! behavior.
//!
//! # Unimplemented (no feature — always available)
//!
//! `unimplemented` holds the fail-closed placeholders for the ADR 0013 transport-security seam
//! ([`TrustStore`](crate::TrustStore), [`KeyAgreement`](crate::KeyAgreement),
//! [`KeyDerivation`](crate::KeyDerivation), [`RecordCipher`](crate::RecordCipher),
//! [`TrustedTime`](crate::TrustedTime)). Every operation refuses. It carries no dependencies and no
//! feature gate, so the seam has a compiling inhabitant in every build configuration.
//!
//! # Software (`feature = "software"`, ON by default)
//!
//! `software` is the pure-Rust, no-system-library backend for the SOFTWARE tier (private keys in
//! normal-world memory — never claims hardware/FIPS custody). See its module doc for the algorithm
//! choice, key-arena design, and token format.
//!
//! # wolfCrypt (`feature = "wolfcrypt"`)
//!
//! `wolfcrypt` is the concrete backend for this crate. It binds against wolfCrypt's **RustCrypto
//! trait surface** (`digest::Digest`, `rand_core`'s RNG traits, `signature::Verifier`) rather than
//! wolfCrypt-internal types, so the delegation logic stays correct even if a concrete type path
//! needs a one-line fix on the build host.
//!
//! **It composes with, and does not replace, the posture toggles:**
//! * `fips` selects wolfCrypt **FIPS mode** — the FIPS 140-3 validated wolfCrypt module, approved
//!   algorithms only, power-on KATs. The approved-algorithm gate ([`Provider::check_approved`]) and
//!   the self-test hook ([`crate::provider`]) still own that policy; wolfCrypt supplies the
//!   validated primitives underneath.
//! * `trustzone` is orthogonal: wolfCrypt software keys are **software custody**
//!   ([`KeyCustody::SoftwareMemory`](crate::KeyCustody)) — the `WolfKeyStore`
//!   reports this honestly and never claims hardware backing.
//!
//! **Build ledger.** The `wolfcrypt` feature REQUIRES a prebuilt wolfSSL 5.9.1+ library at link time
//! (`wolfssl-wolfcrypt` links it, NOT vendored); for the `fips` tier it requires a FIPS 140-3
//! validated wolfCrypt module. It
//! therefore does not link in the default CI/dev environment — it is a **separate build ledger**,
//! analogous to the Zephyr `west` target. Enabling the seam here (feature metadata + off-by-default
//! plumbing) does not link wolfCrypt; only `--features wolfcrypt` on a wolfSSL-equipped host does.
//!
//! [`Provider::check_approved`]: crate::Provider::check_approved

#[cfg(feature = "software")]
pub mod software;
pub mod unimplemented;
#[cfg(feature = "wolfcrypt")]
pub mod wolfcrypt;
