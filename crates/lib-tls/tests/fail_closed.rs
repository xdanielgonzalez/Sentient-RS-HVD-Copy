//! The fail-closed contract, asserted method by method.
//!
//! ADR 0013's skeleton has one non-negotiable safety property: **no code path in `lib-tls` returns a
//! success, authorized, verified or handshake-complete result.** A `verify_client_cert` that returns
//! `Ok(())` is far more dangerous than no function at all — it looks implemented, it passes a
//! happy-path test, and it silently authorizes everyone. Review alone is not a durable guard against
//! someone "helpfully" finishing a stub, so the property is pinned here instead.
//!
//! When WP-3 lands, these assertions are what must be *deliberately* rewritten. That is the point:
//! turning a refusal into a success should require editing a test that says, in words, why it was a
//! refusal.
//!
//! The second thing this file proves is that [`SecureProvider`]'s blanket impl actually works — the
//! double below implements the eight seam traits and nothing else, and the fact that it can be
//! handed to `TlsServerStage::new` is the whole aggregation claim in ADR 0013 §1 under test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_security::{
    Algorithm, Digest, Hasher, KexGroup, KeyAgreement, KeyCustody, KeyDerivation, KeyHandle,
    KeyPurpose, KeyStore, PeerCredential, PeerIdentity, RecordCipher, RecordProtection,
    Result as SecResult, Rng, SecurityError, TimeTrust, TrustModel, TrustStore, TrustedTime,
    UnixSeconds, Verifier,
};
use lib_tls::{
    PinnedSpkiClientVerifier, SeamCryptoProvider, SeamTimeProvider, TlsError, TlsServerStage,
};

/// A seam backend that implements every trait in the `SecureProvider` bound list and refuses every
/// operation. It is not a mock of a working provider — there is nothing yet to mock. It exists to
/// satisfy the bounds so the crate's own refusals can be observed, and it mirrors the posture the
/// real seam has today: ADR 0013 §Context records that the `veneers` feature is off in every
/// shipping build and that every seal through the veneer currently fails closed anyway.
#[derive(Default)]
struct RefusingProvider;

impl TrustStore for RefusingProvider {
    fn custody(&self) -> KeyCustody {
        KeyCustody::SoftwareMemory
    }
    fn model(&self) -> TrustModel {
        TrustModel::PinnedSpki
    }
    fn anchor_count(&self) -> usize {
        0
    }
    fn verify_peer(
        &self,
        _credential: &PeerCredential<'_>,
        _now: Option<UnixSeconds>,
        _out: &mut PeerIdentity,
    ) -> SecResult<()> {
        Err(SecurityError::NotImplemented)
    }
    fn device_identity(&self, _purpose: KeyPurpose) -> SecResult<KeyHandle> {
        Err(SecurityError::NotImplemented)
    }
    fn device_credential(&self, _out: &mut [u8]) -> SecResult<usize> {
        Err(SecurityError::NotImplemented)
    }
}

impl KeyStore for RefusingProvider {
    fn custody(&self) -> KeyCustody {
        KeyCustody::SoftwareMemory
    }
    fn generate(&mut self, _algorithm: Algorithm) -> SecResult<KeyHandle> {
        Err(SecurityError::NotImplemented)
    }
    fn import_private(&mut self, _algorithm: Algorithm, _bytes: &[u8]) -> SecResult<KeyHandle> {
        Err(SecurityError::NotImplemented)
    }
    fn export_public(&self, _handle: KeyHandle, _out: &mut [u8]) -> SecResult<usize> {
        Err(SecurityError::NotImplemented)
    }
    fn sign(&self, _handle: KeyHandle, _message: &[u8], _out: &mut [u8]) -> SecResult<usize> {
        Err(SecurityError::NotImplemented)
    }
}

impl KeyAgreement for RefusingProvider {
    fn custody(&self) -> KeyCustody {
        KeyCustody::SoftwareMemory
    }
    fn generate_ephemeral(&mut self, _group: KexGroup) -> SecResult<KeyHandle> {
        Err(SecurityError::NotImplemented)
    }
    fn public_share(&self, _handle: KeyHandle, _out: &mut [u8]) -> SecResult<usize> {
        Err(SecurityError::NotImplemented)
    }
    fn agree(&mut self, _handle: KeyHandle, _peer_share: &[u8]) -> SecResult<KeyHandle> {
        Err(SecurityError::NotImplemented)
    }
}

impl KeyDerivation for RefusingProvider {
    fn extract(
        &mut self,
        _salt: Option<KeyHandle>,
        _ikm: Option<KeyHandle>,
    ) -> SecResult<KeyHandle> {
        Err(SecurityError::NotImplemented)
    }
    fn expand_label(
        &mut self,
        _secret: KeyHandle,
        _label: &[u8],
        _context: &[u8],
        _len: usize,
    ) -> SecResult<KeyHandle> {
        Err(SecurityError::NotImplemented)
    }
    fn expand_to_bytes(
        &mut self,
        _secret: KeyHandle,
        _label: &[u8],
        _context: &[u8],
        _out: &mut [u8],
    ) -> SecResult<usize> {
        Err(SecurityError::NotImplemented)
    }
}

impl RecordCipher for RefusingProvider {
    fn custody(&self) -> KeyCustody {
        KeyCustody::SoftwareMemory
    }
    fn protection(&self) -> RecordProtection {
        RecordProtection::SecureWorld
    }
    fn seal_record(
        &mut self,
        _key: KeyHandle,
        _seq: u64,
        _aad: &[u8],
        _buf: &mut [u8],
        _len: usize,
    ) -> SecResult<usize> {
        Err(SecurityError::NotImplemented)
    }
    fn open_record(
        &mut self,
        _key: KeyHandle,
        _seq: u64,
        _aad: &[u8],
        _buf: &mut [u8],
        _len: usize,
    ) -> SecResult<usize> {
        Err(SecurityError::NotImplemented)
    }
}

impl Verifier for RefusingProvider {
    fn verify(
        &self,
        _algorithm: Algorithm,
        _public_key: &[u8],
        _message: &[u8],
        _signature: &[u8],
    ) -> SecResult<()> {
        Err(SecurityError::NotImplemented)
    }
}

impl Hasher for RefusingProvider {
    fn digest(&self) -> Digest {
        Digest::Sha256
    }
    fn update(&mut self, _data: &[u8]) {}
    fn finish(&mut self, _out: &mut [u8]) -> SecResult<usize> {
        Err(SecurityError::NotImplemented)
    }
}

impl Rng for RefusingProvider {
    fn fill(&mut self, _out: &mut [u8]) -> SecResult<()> {
        Err(SecurityError::NotImplemented)
    }
}

/// A clock seam that reports, honestly, that it has no idea what time it is. `TrustedTime` is
/// deliberately outside the `SecureProvider` bound list (ADR 0013 §1) so a caller cannot pick up a
/// timestamp incidentally while reaching for crypto.
#[derive(Default)]
struct UnavailableClock;

impl TrustedTime for UnavailableClock {
    fn trust(&self) -> TimeTrust {
        TimeTrust::Unavailable
    }
    fn now(&self) -> SecResult<UnixSeconds> {
        Err(SecurityError::NotImplemented)
    }
    fn uptime_millis(&self) -> u64 {
        0
    }
}

/// The aggregation claim: eight traits implemented, `SecureProvider` obtained for free through the
/// blanket impl, and one generic at the call site. If this compiles, ADR 0013 §1's "no `CryptoOps`
/// god-trait" decision holds.
#[test]
fn secure_provider_is_satisfied_by_the_eight_seam_traits() {
    let stage = TlsServerStage::new(RefusingProvider);
    assert_eq!(TrustStore::anchor_count(stage.provider()), 0);
}

#[test]
fn tls_server_stage_refuses_every_operation() {
    let mut stage = TlsServerStage::new(RefusingProvider);
    let mut buf = [0_u8; 64];

    // A plausible TLS 1.3 ClientHello record prefix — nothing about well-formed input may change
    // the answer, because nothing is implemented behind it.
    assert_eq!(
        stage.feed_incoming(&[0x16, 0x03, 0x01]),
        Err(TlsError::Unimplemented)
    );
    assert_eq!(stage.drain_outgoing(&mut buf), Err(TlsError::Unimplemented));
    assert_eq!(
        stage.write_plaintext(b"telemetry"),
        Err(TlsError::Unimplemented)
    );
    assert_eq!(stage.read_plaintext(&mut buf), Err(TlsError::Unimplemented));
    // `matches!` rather than `assert_eq!`: the `Ok` type is a `lib-security` seam type whose derives
    // are that crate's business, not this test's.
    assert!(matches!(stage.peer_anchor(), Err(TlsError::Unimplemented)));
}

/// The signature cannot carry an error, so the stub returns the answer that keeps the connection
/// unusable. `false` would announce a completed handshake — that a peer had been authenticated.
#[test]
fn tls_server_stage_never_reports_a_completed_handshake() {
    let stage = TlsServerStage::new(RefusingProvider);
    assert!(
        stage.is_handshaking(),
        "a stub must never report the handshake complete"
    );
}

/// The load-bearing test in this file. `Ok(())` from either method authorizes every client that
/// connects.
#[test]
fn client_verifier_authorizes_nobody() {
    let verifier = PinnedSpkiClientVerifier::new(RefusingProvider);

    assert_eq!(
        verifier.verify_client_cert(b"not-a-certificate", &[], None),
        Err(TlsError::Unimplemented),
    );
    // Even a well-formed-looking DER SEQUENCE header and a plausible intermediate list must not
    // change the answer: no comparison is implemented, so there is nothing that could have passed.
    assert_eq!(
        verifier.verify_client_cert(&[0x30, 0x82, 0x01, 0x0a], &[&[0x30, 0x82]], None),
        Err(TlsError::Unimplemented),
    );
    assert_eq!(
        verifier.verify_tls13_signature(b"transcript", b"cert", b"signature"),
        Err(TlsError::Unimplemented),
    );
}

/// ADR 0013 §6: mTLS is mandatory and is the only authentication mechanism on the MCU broker. This
/// is not configurable, so there is no build in which it can be relaxed by accident.
#[test]
fn client_auth_is_mandatory_and_not_configurable() {
    let verifier = PinnedSpkiClientVerifier::new(RefusingProvider);
    assert!(verifier.client_auth_mandatory());
}

#[test]
fn crypto_provider_refuses_every_primitive() {
    let mut provider = SeamCryptoProvider::new(RefusingProvider);
    let mut out = [0_u8; 32];

    // A random source that quietly returned zeroes would compromise every key derived from it while
    // looking healthy. It errors, and `out` is left untouched.
    assert_eq!(provider.fill_random(&mut out), Err(TlsError::Unimplemented));
    assert_eq!(out, [0_u8; 32]);

    assert_eq!(
        provider.verify_signature(b"public-key", b"message", b"signature"),
        Err(TlsError::Unimplemented),
    );
    assert!(matches!(
        provider.device_signing_key(),
        Err(TlsError::Unimplemented)
    ));
}

/// ADR 0013 §5 specifies epoch-on-`Unavailable` for the *implemented* provider. The skeleton must
/// not pre-empt that with a plausible timestamp a caller cannot distinguish from a working clock.
#[test]
fn time_provider_refuses_rather_than_inventing_a_timestamp() {
    let time = SeamTimeProvider::new(UnavailableClock);

    assert!(matches!(time.now(), Err(TlsError::Unimplemented)));
    assert_eq!(time.uptime_millis(), Err(TlsError::Unimplemented));
    // The clock's own honesty accessor is passed through unchanged — this type never upgrades it.
    assert!(matches!(time.clock().trust(), TimeTrust::Unavailable));
}

/// A seam refusal stays legible as a seam refusal instead of being flattened into a generic
/// handshake failure — but it still lands on the error side, which is the property that matters.
#[test]
fn seam_errors_are_carried_through_not_flattened() {
    let err: TlsError = SecurityError::KeyExportForbidden.into();
    assert_eq!(err, TlsError::Security(SecurityError::KeyExportForbidden));
}
