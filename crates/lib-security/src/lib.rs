//! `lib-security` — auth tokens, certificates, and cryptography behind one provider seam.
//!
//! This crate is the security boundary for the Sentient METEOR adapter. It abstracts crypto so two
//! posture toggles are compile-time features, honored by construction rather than by discipline:
//!
//! * **FIPS** (`feature = "fips"`) — cryptography runs inside a FIPS 140-3 validated module;
//!   power-on self-tests (KATs) run at [`Provider::new`]; only approved algorithms are reachable
//!   (a non-approved primitive is [`SecurityError::NotApproved`], never a silent downgrade).
//! * **TrustZone / secure computing** (`feature = "trustzone"`) — private keys live in the secure
//!   world / a secure element / a TPM and are referenced by opaque [`KeyHandle`]s; key bytes never
//!   return to normal world. With it off, custody is software memory — a **strictly lower tier**,
//!   reported by [`SecurityProfile`], never silently equivalent.
//!
//! Design invariants (see `docs/security-integration-map.md` for where each plugs into the system):
//! * **Fail closed.** Every verify returns `Result`; `Ok` means *proven*, any `Err` means *deny*.
//!   Unbound backends return [`SecurityError::NotImplemented`], not a fake success.
//! * **No private-key export.** [`KeyStore`] signs with a handle and exports only public keys —
//!   there is deliberately no `export_private`, so the type system forbids leaking key material.
//! * **`no_std` core.** rng/hash/sign over slices + opaque handles need no allocator (the MCU
//!   forward path); host-only X.509 chain verification is gated behind `feature = "std"`.
//!
//! ## The transport-security seam (ADR 0013)
//!
//! [`TrustStore`], [`KeyAgreement`], [`KeyDerivation`], [`RecordCipher`] and [`TrustedTime`] extend
//! the same seam to a TLS 1.3 server leg and to the DDS/TMS identity binding — **one trust store,
//! in the Secure world, behind a trait, serving both**. They are five separate traits, deliberately
//! not a `CryptoOps` god-trait re-declaring the sign/verify/random/digest operations above. Their
//! only implementations today are the fail-closed placeholders in
//! [`backends::unimplemented`] and the stub [`extract_spki`]; the seam is defined so a
//! chip migration changes the backend, not the architecture. See
//! `docs/decisions/0013-transport-security-trust-store.md`.
//!
//! Concrete cryptographic backends are wired through the enforced planner→scaffold→fill→gate
//! pipeline against the integration map; this crate defines and tests the seam and its boundary
//! rules (tier reporting, the FIPS approved-algorithm gate, the no-export invariant, fail-closed
//! defaults).
#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

pub mod error;
pub mod profile;
pub mod provider;
pub mod traits;

/// Concrete cryptographic backends behind the seam. Most are feature-gated: `backends::software`
/// is the pure-Rust software backend (`feature = "software"`, ON by default — no system library,
/// no_std/no_alloc) and `backends::wolfcrypt` the wolfCrypt backend (`feature = "wolfcrypt"`, OFF
/// by default, a separate build ledger requiring system wolfSSL 5.9.0+).
/// `backends::unimplemented` is **ungated** — it is pure refusal with no dependencies, so the
/// ADR 0013 transport seam has a compiling, fail-closed inhabitant in every configuration.
pub mod backends;

mod impl_spki_extract;

pub use error::{Result, SecurityError};
pub use impl_spki_extract::{extract_spki, SPKI_MAX_LEN};
pub use profile::{CryptoValidation, KeyCustody, KeyPurpose, SecurityProfile};
pub use provider::{approved_under_fips, Provider};
pub use traits::{
    Algorithm, AnchorId, Digest, Hasher, KexGroup, KeyAgreement, KeyDerivation, KeyHandle,
    KeyStore, PeerCredential, PeerIdentity, RecordCipher, RecordProtection, Rng, SessionCipher,
    TimeTrust, TokenClaims, TokenPolicy, TokenVerifier, TrustModel, TrustStore, TrustedTime,
    UnixSeconds, Verifier, PEER_SUBJECT_MAX,
};

#[cfg(feature = "std")]
pub use traits::CertVerifier;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// The one build-specific check: `SecurityProfile::active()` reflects the compile-time feature
    /// toggles. The behavioral grid (tiers × algorithms × errors) lives in `tests/matrix.rs`
    /// (rstest), which supersedes the old hand-written unit tests.
    #[test]
    fn active_profile_matches_features() {
        let p = SecurityProfile::active();
        assert_eq!(p.fips_enabled(), cfg!(feature = "fips"));
        assert_eq!(p.secure_world_enabled(), cfg!(feature = "trustzone"));
        assert!(!p.tier_slug().is_empty());
    }
}
