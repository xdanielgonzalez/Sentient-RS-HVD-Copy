//! Prove-out test matrix for the security seam, built on `rstest` fixtures + parameterized cases.
//!
//! The crate's whole purpose is that every security *tier* (FIPS × TrustZone × secure-element) and
//! every *algorithm* behaves correctly and fail-closed. Rather than a handful of hand-written
//! asserts, this exercises the full grid:
//!   * tier matrix — the 4 legacy (validation × uniform custody) combinations → reported posture,
//!   * mixed-custody axis — {none, trustzone, secure-element, secure-element+trustzone} × fips,
//!     covering per-purpose (identity vs. session) custody and the weakest-wins summary,
//!   * `KeyCustody::weaker_of` ordering and `custody_warning`'s per-purpose naming,
//!   * FIPS approved-algorithm matrix — algorithm × fips → allow/deny,
//!   * provider-consistency — the live `Provider` gate agrees with the pure rule,
//!   * error surface — every `SecurityError` is a non-empty deny signal.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_security::{
    approved_under_fips, Algorithm, CryptoValidation, KeyCustody, KeyPurpose, Provider,
    SecurityError, SecurityProfile,
};
use rstest::{fixture, rstest};

// ---- fixtures -------------------------------------------------------------------------------

/// A profile at an explicit tier — the clean fixture the tier cases build on. Parameterizable via
/// rstest `#[with(..)]` at the call site. `identity_custody`/`session_custody` default to the
/// legacy single-tier shape (both the same) so existing `#[with(validation, custody)]`-style
/// overrides that only vary one axis still read naturally; per-purpose mixed-custody cases pass
/// both explicitly.
#[fixture]
fn profile(
    #[default(CryptoValidation::Standard)] validation: CryptoValidation,
    #[default(KeyCustody::SoftwareMemory)] identity_custody: KeyCustody,
    #[default(KeyCustody::SoftwareMemory)] session_custody: KeyCustody,
) -> SecurityProfile {
    SecurityProfile {
        validation,
        identity_custody,
        session_custody,
    }
}

/// The provider for the build's active tier. In the default (non-FIPS) build this constructs; under
/// a `fips` build `Provider::new` self-test-fails by design, so provider cases are guarded to the
/// non-FIPS build (the FIPS gate itself is covered purely below, independent of the build).
#[fixture]
fn active_provider() -> Provider {
    Provider::new().expect("provider constructs at the active (non-FIPS) tier")
}

// ---- tier matrix ----------------------------------------------------------------------------

#[rstest]
#[case(
    CryptoValidation::Standard,
    KeyCustody::SoftwareMemory,
    "standard+software",
    false,
    false
)]
#[case(
    CryptoValidation::Standard,
    KeyCustody::SecureWorld,
    "standard+trustzone",
    false,
    true
)]
#[case(
    CryptoValidation::Fips140_3,
    KeyCustody::SoftwareMemory,
    "fips+software",
    true,
    false
)]
#[case(
    CryptoValidation::Fips140_3,
    KeyCustody::SecureWorld,
    "fips+trustzone",
    true,
    true
)]
fn tier_matrix(
    #[case] validation: CryptoValidation,
    #[case] custody: KeyCustody,
    #[case] slug: &str,
    #[case] fips: bool,
    #[case] secure_world: bool,
) {
    // Uniform custody (identity == session) — the legacy single-tier shape these 4 cases model.
    let p = SecurityProfile {
        validation,
        identity_custody: custody,
        session_custody: custody,
    };
    assert_eq!(p.tier_slug(), slug);
    assert_eq!(p.fips_enabled(), fips);
    assert_eq!(p.secure_world_enabled(), secure_world);
    assert_eq!(p.custody(KeyPurpose::Identity), custody);
    assert_eq!(p.custody(KeyPurpose::Session), custody);
    assert_eq!(p.weakest_custody(), custody);
    // Honesty accessor: a software-custody tier MUST surface a warning; secure-world must not.
    assert_eq!(
        p.custody_warning().is_some(),
        custody == KeyCustody::SoftwareMemory,
        "custody_warning must flag software custody and only software custody",
    );
}

// ---- per-purpose (mixed) custody matrix ------------------------------------------------------
//
// The axis this story adds: {none, trustzone, secure-element, secure-element+trustzone} x
// {fips off, fips on}. Per `SecurityProfile::active`'s derivation, `secure-element` raises ONLY
// identity custody (to `SecureElement`); `trustzone` raises ONLY session custody (to
// `SecureWorld`) — the two combine independently. These cases hand-construct the profile the way
// each feature combo would resolve to, rather than requiring a separate build per combo.
#[rstest]
// -- fips off --
#[case(
    CryptoValidation::Standard,
    KeyCustody::SoftwareMemory,
    KeyCustody::SoftwareMemory,
    "standard+software",
    false
)] // none
#[case(
    CryptoValidation::Standard,
    KeyCustody::SecureWorld,
    KeyCustody::SecureWorld,
    "standard+trustzone",
    true
)] // trustzone
#[case(
    CryptoValidation::Standard,
    KeyCustody::SecureElement,
    KeyCustody::SoftwareMemory,
    "standard+software",
    false
)] // secure-element alone: identity hardware, session still software — weakest wins, no slug bump
#[case(
    CryptoValidation::Standard,
    KeyCustody::SecureElement,
    KeyCustody::SecureWorld,
    "standard+trustzone",
    true
)]
// secure-element + trustzone: both hardware-backed (different tiers) — reports as trustzone
// -- fips on --
#[case(
    CryptoValidation::Fips140_3,
    KeyCustody::SoftwareMemory,
    KeyCustody::SoftwareMemory,
    "fips+software",
    false
)]
#[case(
    CryptoValidation::Fips140_3,
    KeyCustody::SecureWorld,
    KeyCustody::SecureWorld,
    "fips+trustzone",
    true
)]
#[case(
    CryptoValidation::Fips140_3,
    KeyCustody::SecureElement,
    KeyCustody::SoftwareMemory,
    "fips+software",
    false
)]
#[case(
    CryptoValidation::Fips140_3,
    KeyCustody::SecureElement,
    KeyCustody::SecureWorld,
    "fips+trustzone",
    true
)]
fn mixed_custody_axis_matrix(
    #[case] validation: CryptoValidation,
    #[case] identity_custody: KeyCustody,
    #[case] session_custody: KeyCustody,
    #[case] slug: &str,
    #[case] secure_world: bool,
) {
    let p = SecurityProfile {
        validation,
        identity_custody,
        session_custody,
    };
    assert_eq!(p.tier_slug(), slug);
    assert_eq!(p.secure_world_enabled(), secure_world);
    assert_eq!(p.custody(KeyPurpose::Identity), identity_custody);
    assert_eq!(p.custody(KeyPurpose::Session), session_custody);
    assert_eq!(
        p.weakest_custody(),
        KeyCustody::weaker_of(identity_custody, session_custody)
    );
}

/// `secure-element` alone must never overstate protection: the slug and `secure_world_enabled`
/// must both read exactly as if `secure-element` were never enabled, because the session key is
/// still software-custodied. This is the specific "weakest-wins" claim the story is about.
#[rstest]
fn secure_element_alone_does_not_upgrade_the_summary() {
    let se_alone = SecurityProfile {
        validation: CryptoValidation::Standard,
        identity_custody: KeyCustody::SecureElement,
        session_custody: KeyCustody::SoftwareMemory,
    };
    let none = SecurityProfile {
        validation: CryptoValidation::Standard,
        identity_custody: KeyCustody::SoftwareMemory,
        session_custody: KeyCustody::SoftwareMemory,
    };
    assert_eq!(se_alone.tier_slug(), none.tier_slug());
    assert_eq!(se_alone.secure_world_enabled(), none.secure_world_enabled());
    // But `custody(Identity)` still tells the truth for a caller that asks the specific question.
    assert_eq!(
        se_alone.custody(KeyPurpose::Identity),
        KeyCustody::SecureElement
    );
    assert_ne!(
        none.custody(KeyPurpose::Identity),
        KeyCustody::SecureElement
    );
}

// ---- KeyCustody::weaker_of ordering ----------------------------------------------------------

#[rstest]
#[case(
    KeyCustody::SoftwareMemory,
    KeyCustody::SoftwareMemory,
    KeyCustody::SoftwareMemory
)]
#[case(
    KeyCustody::SoftwareMemory,
    KeyCustody::SecureWorld,
    KeyCustody::SoftwareMemory
)]
#[case(
    KeyCustody::SoftwareMemory,
    KeyCustody::SecureElement,
    KeyCustody::SoftwareMemory
)]
#[case(
    KeyCustody::SecureWorld,
    KeyCustody::SoftwareMemory,
    KeyCustody::SoftwareMemory
)]
#[case(
    KeyCustody::SecureWorld,
    KeyCustody::SecureWorld,
    KeyCustody::SecureWorld
)]
#[case(
    KeyCustody::SecureWorld,
    KeyCustody::SecureElement,
    KeyCustody::SecureWorld
)]
#[case(
    KeyCustody::SecureElement,
    KeyCustody::SoftwareMemory,
    KeyCustody::SoftwareMemory
)]
#[case(
    KeyCustody::SecureElement,
    KeyCustody::SecureWorld,
    KeyCustody::SecureWorld
)]
#[case(
    KeyCustody::SecureElement,
    KeyCustody::SecureElement,
    KeyCustody::SecureElement
)]
fn weaker_of_ordering(#[case] a: KeyCustody, #[case] b: KeyCustody, #[case] expected: KeyCustody) {
    assert_eq!(
        KeyCustody::weaker_of(a, b),
        expected,
        "weaker_of({a:?}, {b:?})"
    );
    // Commutative — the strength order is total, so argument order must not matter.
    assert_eq!(
        KeyCustody::weaker_of(b, a),
        expected,
        "weaker_of({b:?}, {a:?})"
    );
}

// ---- custody_warning: names the weak purpose(s) ------------------------------------------------

#[rstest]
#[case(KeyCustody::SoftwareMemory, KeyCustody::SoftwareMemory, &["identity", "session", "BOTH"])]
#[case(KeyCustody::SoftwareMemory, KeyCustody::SecureWorld, &["identity"])]
#[case(KeyCustody::SecureElement, KeyCustody::SoftwareMemory, &["session"])]
#[case(KeyCustody::SecureWorld, KeyCustody::SecureWorld, &[])]
#[case(KeyCustody::SecureElement, KeyCustody::SecureWorld, &[])]
fn custody_warning_names_the_weak_purpose(
    #[case] identity_custody: KeyCustody,
    #[case] session_custody: KeyCustody,
    #[case] must_contain: &[&str],
) {
    let p = SecurityProfile {
        validation: CryptoValidation::Standard,
        identity_custody,
        session_custody,
    };
    let warning = p.custody_warning();
    if must_contain.is_empty() {
        assert!(warning.is_none(), "expected no warning, got {warning:?}");
    } else {
        let text = warning.expect("expected a warning naming the weak purpose(s)");
        for needle in must_contain {
            assert!(
                text.contains(needle),
                "warning {text:?} must mention {needle:?}"
            );
        }
    }
}

// ---- fixture in action ----------------------------------------------------------------------

/// The default `profile` fixture is the software tier — it must report as such and warn.
#[rstest]
fn fixture_default_is_software_and_warns(profile: SecurityProfile) {
    assert_eq!(profile.tier_slug(), "standard+software");
    assert!(profile.custody_warning().is_some());
}

/// The same fixture, overridden at the call site — a secure-world tier (both purposes) must not
/// warn.
#[rstest]
fn fixture_override_to_secure_world(
    #[with(
        CryptoValidation::Fips140_3,
        KeyCustody::SecureWorld,
        KeyCustody::SecureWorld
    )]
    profile: SecurityProfile,
) {
    assert_eq!(profile.tier_slug(), "fips+trustzone");
    assert!(profile.custody_warning().is_none());
}

/// Overridden to mixed custody (identity in a secure element, session still software) — the
/// fixture composes with per-purpose overrides the same way it always composed with the single
/// legacy custody override.
#[rstest]
fn fixture_override_to_mixed_custody(
    #[with(
        CryptoValidation::Standard,
        KeyCustody::SecureElement,
        KeyCustody::SoftwareMemory
    )]
    profile: SecurityProfile,
) {
    assert_eq!(profile.tier_slug(), "standard+software");
    assert!(profile.custody_warning().is_some());
    assert_eq!(
        profile.custody(KeyPurpose::Identity),
        KeyCustody::SecureElement
    );
    assert_eq!(
        profile.custody(KeyPurpose::Session),
        KeyCustody::SoftwareMemory
    );
}

// ---- FIPS approved-algorithm matrix (algorithm × fips) --------------------------------------

#[rstest]
fn approved_algorithm_grid(
    #[values(
        Algorithm::Ed25519,
        Algorithm::EcdsaP256Sha256,
        Algorithm::EcdsaP384Sha384,
        Algorithm::RsaPkcs1Sha256
    )]
    algorithm: Algorithm,
    #[values(false, true)] fips: bool,
) {
    let expect_ok = !fips || algorithm.is_fips_approved();
    assert_eq!(approved_under_fips(algorithm, fips).is_ok(), expect_ok);
    if !expect_ok {
        // The only non-approved listed algorithm under FIPS is Ed25519; it must fail CLOSED with
        // the exact NotApproved error, never a silent downgrade.
        assert_eq!(algorithm, Algorithm::Ed25519);
        assert_eq!(
            approved_under_fips(algorithm, fips),
            Err(SecurityError::NotApproved)
        );
    }
}

#[rstest]
#[case(Algorithm::Ed25519, false)]
#[case(Algorithm::EcdsaP256Sha256, true)]
#[case(Algorithm::EcdsaP384Sha384, true)]
#[case(Algorithm::RsaPkcs1Sha256, true)]
fn fips_approved_classification(#[case] algorithm: Algorithm, #[case] approved: bool) {
    assert_eq!(algorithm.is_fips_approved(), approved);
}

// ---- provider consistency (fixture + values) ------------------------------------------------

#[rstest]
#[cfg(not(feature = "fips"))]
fn provider_gate_matches_pure_rule(
    active_provider: Provider,
    #[values(
        Algorithm::Ed25519,
        Algorithm::EcdsaP256Sha256,
        Algorithm::EcdsaP384Sha384,
        Algorithm::RsaPkcs1Sha256
    )]
    algorithm: Algorithm,
) {
    let fips = active_provider.profile().fips_enabled();
    // The live provider gate must agree with the pure rule for every algorithm.
    assert_eq!(
        active_provider.check_approved(algorithm).is_ok(),
        approved_under_fips(algorithm, fips).is_ok(),
    );
}

#[rstest]
#[cfg(not(feature = "fips"))]
fn non_fips_provider_permits_every_algorithm(active_provider: Provider) {
    let all = [
        Algorithm::Ed25519,
        Algorithm::EcdsaP256Sha256,
        Algorithm::EcdsaP384Sha384,
        Algorithm::RsaPkcs1Sha256,
    ];
    for alg in all {
        assert!(
            active_provider.check_approved(alg).is_ok(),
            "{alg:?} must pass without FIPS"
        );
    }
}

// ---- error surface: every variant is a non-empty deny signal --------------------------------

#[rstest]
#[case(SecurityError::NotImplemented)]
#[case(SecurityError::NotApproved)]
#[case(SecurityError::SelfTestFailed)]
#[case(SecurityError::EntropyFailure)]
#[case(SecurityError::KeyNotFound)]
#[case(SecurityError::KeyExportForbidden)]
#[case(SecurityError::VerificationFailed)]
#[case(SecurityError::Expired)]
#[case(SecurityError::UntrustedChain)]
#[case(SecurityError::IdentityMismatch)]
#[case(SecurityError::BufferTooSmall)]
#[case(SecurityError::Malformed)]
#[case(SecurityError::Backend)]
fn error_is_deny_signal(#[case] err: SecurityError) {
    assert!(!format!("{err}").is_empty(), "Display must carry a message");
    assert!(!format!("{err:?}").is_empty(), "Debug must render");
}
