//! Steps for the security feature area — `security.feature` (the software crypto backend: keys,
//! signatures, tiers, the FIPS gate, `KeyStoreTlsIdentity`) and `authz.feature` (the token verifier
//! accept/reject matrix + the `seam_auth` subject/username rule). NO crypto, token framing, or
//! MQTT-broker logic is re-implemented here — every signing, verification, tier, FIPS-gate, and
//! TLS-identity operation below is a direct call into the real `lib_security`, `lib_net::link`,
//! and `test_kit` crates. What lives here is Gherkin<->Rust plumbing only: building
//! fixtures, mapping a Gherkin algorithm/outcome name to the typed enum, and asserting on results —
//! never re-deriving ECDSA math, token framing, or the FIPS approved-algorithm rule.
//!
//! Sources: `docs/security.md`, `docs/security-integration-map.md` (see each `.feature` file's own
//! header for the exact section citations).
//!
//! Test code only; freely uses unwrap/expect/panic (the workspace lint gate denies these in
//! production logic, not tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use cucumber::{given, then, when};

use rustls::pki_types::CertificateDer;
use rustls::SignatureScheme;

use lib_mqtt::broker::BrokerConfig;
use lib_net::link::tls_identity::{KeyStoreTlsIdentity, TlsIdentity};
use lib_security::backends::software::{
    self_test, sign_token, InsecureTestRng, SoftwareKeyStore, SoftwareTokenVerifier,
    SoftwareVerifier, PUBLIC_KEY_LEN, SIGNATURE_LEN, TOKEN_MAX,
};
use lib_security::{
    approved_under_fips, Algorithm, KeyCustody, KeyHandle, KeyStore, SecurityError,
    SecurityProfile, TokenClaims, TokenPolicy, TokenVerifier, Verifier,
};
use test_kit::mqtt::{
    invalid_token, valid_token, FakeTokenVerifier, AUTH_AUDIENCE, AUTH_ISSUER, AUTH_TRUSTED_KEY,
};

use crate::steps::support::error_variant_name;
use crate::SpecWorld;

/// Fixed, non-secret seed for every `InsecureTestRng` this module constructs — deterministic and
/// reproducible (same pattern `lib-security`'s own tests and
/// `test-e2e/tests/{token_auth_software,tls_identity_software}.rs` use). NOT a real key; see the
/// `@insecure-test-rng-non-crypto` scenario in `security.feature`.
const FIXED_SEED: u64 = 0xACCE_5510_0000_0001;

/// A second, distinct seed for a deliberately-different ("untrusted") key in the authz matrix.
const UNTRUSTED_SEED: u64 = 0xACCE_5510_BAD0_0BAD;

#[derive(Debug, Default)]
pub struct SecurityState {
    /// The software key store under test — `Option` so `Given a fresh software key store` can set
    /// it (and the KeyStoreTlsIdentity scenario can `take()` it to move into the identity).
    store: Option<SoftwareKeyStore<InsecureTestRng>>,
    key_handle: Option<KeyHandle>,
    /// Raw SEC1 exported public key bytes, from whichever store most recently generated a key.
    public_key: Option<Vec<u8>>,
    signature: Option<Vec<u8>>,
    /// The message most recently signed — kept alongside `signature` so a later step can verify
    /// against it without re-threading it through every step's parameter list.
    message: Vec<u8>,
    verify_result: Option<lib_security::Result<()>>,
    /// Custody reported by whichever `KeyStore`/`TlsIdentity` a scenario most recently constructed.
    custody: Option<KeyCustody>,
    selftest_result: Option<lib_security::Result<()>>,
    /// Authz matrix: the wire-format token bytes under test (either real, `sign_token`-minted, or
    /// `FakeTokenVerifier`'s shared-secret format).
    token: Option<Vec<u8>>,
    /// Authz matrix: the trusted public key `verify_token` checks the token's signature against.
    trusted_key: Option<Vec<u8>>,
    verify_token_result: Option<lib_security::Result<TokenClaims>>,
}

// ---- security.feature: software key store / signature verification --------------------------

#[given("a fresh software key store")]
fn given_fresh_store(world: &mut SpecWorld) {
    world.security.store = Some(SoftwareKeyStore::new(InsecureTestRng::new(FIXED_SEED)));
}

#[when("the software key store generates a P-256 key")]
fn when_generate_key(world: &mut SpecWorld) {
    let store = world.security.store.as_mut().expect("store initialized");
    let handle = store
        .generate(Algorithm::EcdsaP256Sha256)
        .expect("generate a P-256 key");
    let mut public_key = [0u8; PUBLIC_KEY_LEN];
    let pk_len = store
        .export_public(handle, &mut public_key)
        .expect("export the generated key's public half");
    world.security.key_handle = Some(handle);
    world.security.public_key = Some(public_key[..pk_len].to_vec());
}

#[when(expr = "the software key store signs the message {string} with the generated key")]
fn when_sign_message(world: &mut SpecWorld, text: String) {
    let store = world.security.store.as_ref().expect("store initialized");
    let handle = world.security.key_handle.expect("key generated");
    let message = text.into_bytes();
    let mut signature = [0u8; SIGNATURE_LEN];
    let sig_len = store
        .sign(handle, &message, &mut signature)
        .expect("sign the message");
    world.security.message = message;
    world.security.signature = Some(signature[..sig_len].to_vec());
}

#[when("the software key store signature is tampered with")]
fn when_tamper_signature(world: &mut SpecWorld) {
    let signature = world
        .security
        .signature
        .as_mut()
        .expect("signature computed");
    signature[0] ^= 0xFF;
}

/// Run `SoftwareVerifier` over the scenario's stored public key + signature against `message`,
/// stashing the result in `world.security.verify_result` (the shared scratch field every "verifier
/// accepts/rejects" Then step below reads from) so the reject-matrix assertions all go through
/// [`error_variant_name`], never a hand-checked `Err` variant.
fn run_verify(world: &mut SpecWorld, message: &[u8]) {
    let public_key = world
        .security
        .public_key
        .clone()
        .expect("public key exported");
    let signature = world
        .security
        .signature
        .clone()
        .expect("signature computed");
    world.security.verify_result =
        Some(SoftwareVerifier.verify(Algorithm::EcdsaP256Sha256, &public_key, message, &signature));
}

#[then("the software verifier accepts the signature against the exported public key")]
fn then_signature_accepted(world: &mut SpecWorld) {
    let message = world.security.message.clone();
    run_verify(world, &message);
    let result = world.security.verify_result.as_ref().expect("verify ran");
    assert!(
        result.is_ok(),
        "expected the signature to verify, got {result:?}"
    );
}

#[then("the software verifier rejects the tampered signature with VerificationFailed")]
fn then_tampered_signature_rejected(world: &mut SpecWorld) {
    let message = world.security.message.clone();
    run_verify(world, &message);
    let result = world.security.verify_result.as_ref().expect("verify ran");
    let err = result
        .as_ref()
        .expect_err("expected the tampered signature to be rejected");
    assert_eq!(error_variant_name(err), "VerificationFailed");
}

#[then(
    expr = "the software verifier rejects the signature against the message {string} with VerificationFailed"
)]
fn then_wrong_message_rejected(world: &mut SpecWorld, text: String) {
    run_verify(world, text.as_bytes());
    let result = world.security.verify_result.as_ref().expect("verify ran");
    let err = result
        .as_ref()
        .expect_err("expected verification against the wrong message to be rejected");
    assert_eq!(error_variant_name(err), "VerificationFailed");
}

#[then("the software key store reports SoftwareMemory custody")]
fn then_store_custody(world: &mut SpecWorld) {
    let store = world.security.store.as_ref().expect("store initialized");
    assert_eq!(store.custody(), KeyCustody::SoftwareMemory);
}

// ---- security.feature: tier / custody-warning honesty accessors -------------------------------

#[then("the active security profile carries a custody warning")]
fn then_profile_warns(_world: &mut SpecWorld) {
    assert!(SecurityProfile::active().custody_warning().is_some());
}

#[then(expr = "the active security profile's tier slug is {string}")]
fn then_profile_tier_slug(_world: &mut SpecWorld, slug: String) {
    assert_eq!(SecurityProfile::active().tier_slug(), slug);
}

// ---- security.feature: the FIPS approved-algorithm gate (mirrors matrix.rs's grid) ------------

fn algorithm_by_name(name: &str) -> Algorithm {
    match name {
        "Ed25519" => Algorithm::Ed25519,
        "EcdsaP256Sha256" => Algorithm::EcdsaP256Sha256,
        "EcdsaP384Sha384" => Algorithm::EcdsaP384Sha384,
        "RsaPkcs1Sha256" => Algorithm::RsaPkcs1Sha256,
        other => panic!("unknown algorithm {other:?} in feature file"),
    }
}

#[then(expr = "algorithm {word} under fips {word} is {word}")]
fn then_fips_gate(_world: &mut SpecWorld, algorithm: String, fips: String, outcome: String) {
    let algorithm_enum = algorithm_by_name(&algorithm);
    let fips_enabled = match fips.as_str() {
        "true" => true,
        "false" => false,
        other => panic!("unknown fips value {other:?} in feature file"),
    };
    let result = approved_under_fips(algorithm_enum, fips_enabled);
    match outcome.as_str() {
        "approved" => assert!(
            result.is_ok(),
            "{algorithm} under fips={fips_enabled} should be approved, got {result:?}"
        ),
        "refused" => assert_eq!(
            result,
            Err(SecurityError::NotApproved),
            "{algorithm} under fips={fips_enabled} should be refused"
        ),
        other => panic!("unknown outcome {other:?} in feature file"),
    }
}

// ---- security.feature: the software backend self-test -----------------------------------------

#[when("the software backend self-test runs")]
fn when_selftest(world: &mut SpecWorld) {
    world.security.selftest_result = Some(self_test());
}

#[then("the software backend self-test succeeds")]
fn then_selftest_succeeds(world: &mut SpecWorld) {
    let result = world
        .security
        .selftest_result
        .as_ref()
        .expect("self-test ran");
    assert!(
        result.is_ok(),
        "expected the self-test to succeed, got {result:?}"
    );
}

// ---- security.feature: KeyStoreTlsIdentity over the real software backend ---------------------

#[when(expr = "a KeyStoreTlsIdentity over the software key store signs the message {string}")]
fn when_tls_identity_signs(world: &mut SpecWorld, text: String) {
    let store = world.security.store.take().expect("store initialized");
    let handle = world.security.key_handle.expect("key generated");
    // A placeholder cert chain — this exercises the signing seam (KeyStore -> rustls Signer), not
    // X.509 chain building, mirroring test-e2e/tests/tls_identity_software.rs.
    let placeholder_chain = vec![CertificateDer::from(vec![0x30, 0x00])];
    let identity = KeyStoreTlsIdentity::new(
        Arc::new(store),
        handle,
        Algorithm::EcdsaP256Sha256,
        placeholder_chain,
    );
    world.security.custody = Some(identity.custody());

    let certified_key = identity
        .certified_key()
        .expect("certified_key builds over the real software backend");
    let signer = certified_key
        .key
        .choose_scheme(&[SignatureScheme::ECDSA_NISTP256_SHA256])
        .expect("the software backend's algorithm maps to a scheme rustls offers");

    let message = text.into_bytes();
    let signature = signer
        .sign(&message)
        .expect("KeyStore::sign (real backend) produces a signature, not NotImplemented");
    world.security.message = message;
    world.security.signature = Some(signature);
}

#[then("the KeyStoreTlsIdentity reports SoftwareMemory custody")]
fn then_identity_custody(world: &mut SpecWorld) {
    assert_eq!(world.security.custody, Some(KeyCustody::SoftwareMemory));
}

// ---- authz.feature: SoftwareTokenVerifier accept/reject matrix (real ECDSA-signed tokens) -----

#[given(
    expr = "a software-issued token for subject {string} issued by {string} for audience {string} expiring at {int}"
)]
fn given_signed_token(
    world: &mut SpecWorld,
    subject: String,
    issuer: String,
    audience: String,
    expires_at: i64,
) {
    let mut store = SoftwareKeyStore::new(InsecureTestRng::new(FIXED_SEED));
    let handle = store
        .generate(Algorithm::EcdsaP256Sha256)
        .expect("generate an issuer key");
    let mut public_key = [0u8; PUBLIC_KEY_LEN];
    let pk_len = store
        .export_public(handle, &mut public_key)
        .expect("export the issuer's public key");

    let mut token = [0u8; TOKEN_MAX];
    let len = sign_token(
        &store,
        handle,
        &subject,
        &issuer,
        &audience,
        expires_at as u64,
        &mut token,
    )
    .expect("mint a real, verifiable token");

    world.security.token = Some(token[..len].to_vec());
    world.security.trusted_key = Some(public_key[..pk_len].to_vec());
    world.security.store = Some(store);
    world.security.key_handle = Some(handle);
}

#[given("the token's signature is tampered with")]
fn given_tamper_token(world: &mut SpecWorld) {
    let token = world.security.token.as_mut().expect("token minted");
    let last = token.len() - 1;
    // Flip a hex DIGIT (not an arbitrary byte XOR) so the token stays valid ASCII/hex, matching
    // lib-security's own `tampered_token_is_rejected` test and
    // test-e2e/tests/token_auth_software.rs's tampering approach.
    token[last] = if token[last] == b'0' { b'1' } else { b'0' };
}

#[given("the trusted key is swapped for an untrusted key")]
fn given_untrusted_key(world: &mut SpecWorld) {
    let mut other = SoftwareKeyStore::new(InsecureTestRng::new(UNTRUSTED_SEED));
    let handle = other
        .generate(Algorithm::EcdsaP256Sha256)
        .expect("generate an untrusted key");
    let mut public_key = [0u8; PUBLIC_KEY_LEN];
    let pk_len = other
        .export_public(handle, &mut public_key)
        .expect("export the untrusted public key");
    world.security.trusted_key = Some(public_key[..pk_len].to_vec());
}

#[given(expr = "the token is truncated to {int} bytes")]
fn given_truncate_token(world: &mut SpecWorld, n: i64) {
    let token = world.security.token.as_mut().expect("token minted");
    token.truncate(n as usize);
}

#[when(expr = "the token is verified against issuer {string} audience {string} at time {int}")]
fn when_verify_token(world: &mut SpecWorld, issuer: String, audience: String, now_unix: i64) {
    let token = world.security.token.clone().expect("token minted");
    let trusted_key = world.security.trusted_key.clone().expect("trusted key set");
    let policy = TokenPolicy {
        issuer: &issuer,
        audience: &audience,
        now_unix: now_unix as u64,
    };
    world.security.verify_token_result =
        Some(SoftwareTokenVerifier.verify_token(&token, &trusted_key, &policy));
}

#[then(expr = "the token verification succeeds with subject {string}")]
fn then_verify_succeeds(world: &mut SpecWorld, subject: String) {
    let result = world
        .security
        .verify_token_result
        .as_ref()
        .expect("verification ran");
    let claims = result
        .as_ref()
        .unwrap_or_else(|e| panic!("expected verification to succeed, got {e:?}"));
    assert_eq!(claims.subject.as_str(), subject);
}

#[then(expr = "the token verification fails with {word}")]
fn then_verify_fails(world: &mut SpecWorld, variant: String) {
    let result = world
        .security
        .verify_token_result
        .as_ref()
        .expect("verification ran");
    let err = result.as_ref().expect_err("expected verification to fail");
    assert_eq!(error_variant_name(err), variant);
}

// ---- authz.feature: FakeTokenVerifier (shared-secret test double, seam_auth setup only) -------

#[given(expr = "a fake-verifier token for subject {string}")]
fn given_fake_token(world: &mut SpecWorld, subject: String) {
    world.security.token = Some(valid_token(&subject).into_bytes());
}

#[given(expr = "a fake-verifier token with the wrong shared secret for subject {string}")]
fn given_fake_invalid_token(world: &mut SpecWorld, subject: String) {
    world.security.token = Some(invalid_token(&subject).into_bytes());
}

#[when("the fake-verifier token is verified")]
fn when_fake_verify(world: &mut SpecWorld) {
    let token = world.security.token.clone().expect("token minted");
    let policy = TokenPolicy {
        issuer: AUTH_ISSUER,
        audience: AUTH_AUDIENCE,
        now_unix: 1_000,
    };
    world.security.verify_token_result =
        Some(FakeTokenVerifier.verify_token(&token, AUTH_TRUSTED_KEY, &policy));
}

// ---- authz.feature: the seam_auth `claims.subject == username` rule (lib-mqtt/src/broker.rs
// `seam_auth`, mirrored at the verifier level — never by calling `seam_auth` or a broker) ---------

#[then(
    expr = "applying the seam_auth subject rule against username {string} permits the connection"
)]
fn then_seam_auth_permits(world: &mut SpecWorld, username: String) {
    let result = world
        .security
        .verify_token_result
        .as_ref()
        .expect("verification ran");
    let claims = result
        .as_ref()
        .unwrap_or_else(|e| panic!("expected verification to succeed, got {e:?}"));
    assert!(
        claims.subject.as_str() == username,
        "seam_auth should permit: subject {:?} must equal username {username:?}",
        claims.subject.as_str()
    );
}

#[then(
    expr = "applying the seam_auth subject rule against username {string} refuses the connection"
)]
fn then_seam_auth_refuses(world: &mut SpecWorld, username: String) {
    let result = world
        .security
        .verify_token_result
        .as_ref()
        .expect("verification ran");
    let claims = result
        .as_ref()
        .unwrap_or_else(|e| panic!("expected verification to succeed, got {e:?}"));
    assert!(
        claims.subject.as_str() != username,
        "seam_auth should refuse: subject {:?} must not equal username {username:?}",
        claims.subject.as_str()
    );
}

// ---- authz.feature: fail-closed-by-default broker auth posture --------------------------------

#[then("the broker's default configuration carries no CONNECT authorizer")]
fn then_default_broker_no_auth(_world: &mut SpecWorld) {
    assert!(BrokerConfig::default().auth.is_none());
}
