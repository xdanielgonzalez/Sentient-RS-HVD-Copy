//! Host tests for the ADR 0013 transport-security seam.
//!
//! Two things are proven here, and the second is the point:
//!
//! 1. **The five traits compose.** A single type implementing `TrustStore + KeyAgreement +
//!    KeyDerivation + RecordCipher + TrustedTime` (alongside the pre-existing `KeyStore` /
//!    `Verifier` / `Hasher` / `Rng`) satisfies an aggregation bound with a blanket impl — the
//!    `SecureProvider` shape `lib-tls` will take as its one type parameter (ADR §1). If the traits
//!    had incompatible receivers or overlapping method names, this would not compile.
//! 2. **Every fail-closed stub actually returns `Err`.** `unimplemented_stubs_never_succeed` walks
//!    every operation of every placeholder and asserts a refusal. A stub that returned `Ok(())`
//!    from `verify_peer` would authorize every peer while looking implemented; that is the failure
//!    mode this test exists to make impossible to reintroduce silently.
//!
//! See `docs/decisions/0013-transport-security-trust-store.md`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_security::backends::unimplemented::{
    UnimplementedKeyAgreement, UnimplementedKeyDerivation, UnimplementedRecordCipher,
    UnimplementedTime, UnimplementedTrustStore,
};
use lib_security::{
    extract_spki, AnchorId, KexGroup, KeyAgreement, KeyCustody, KeyDerivation, KeyHandle,
    KeyPurpose, PeerCredential, PeerIdentity, RecordCipher, RecordProtection, SecurityError,
    TimeTrust, TrustModel, TrustStore, TrustedTime, UnixSeconds, PEER_SUBJECT_MAX, SPKI_MAX_LEN,
};

// ---- (a) the traits compose -----------------------------------------------------------------

/// The aggregation shape ADR §1 specifies for `lib-tls`: a marker trait with a blanket impl over
/// the seam traits, so a consumer takes **one** type parameter without a `CryptoOps` god-trait
/// (no new methods, no re-declared operations).
trait TransportProvider:
    TrustStore + KeyAgreement + KeyDerivation + RecordCipher + TrustedTime
{
}
impl<T> TransportProvider for T where
    T: TrustStore + KeyAgreement + KeyDerivation + RecordCipher + TrustedTime
{
}

/// One type carrying all five seams — the shape a `VeneerSecureProvider` will have on the MCU,
/// where a single CMSE veneer surface backs every trait. Built from the placeholders, so it is
/// itself fail-closed.
#[derive(Debug, Default)]
struct CompositeProvider {
    trust: UnimplementedTrustStore,
    kex: UnimplementedKeyAgreement,
    kdf: UnimplementedKeyDerivation,
    records: UnimplementedRecordCipher,
    clock: UnimplementedTime,
}

impl TrustStore for CompositeProvider {
    fn custody(&self) -> KeyCustody {
        self.trust.custody()
    }
    fn model(&self) -> TrustModel {
        self.trust.model()
    }
    fn anchor_count(&self) -> usize {
        self.trust.anchor_count()
    }
    fn verify_peer(
        &self,
        credential: &PeerCredential<'_>,
        now: Option<UnixSeconds>,
        out: &mut PeerIdentity,
    ) -> lib_security::Result<()> {
        self.trust.verify_peer(credential, now, out)
    }
    fn device_identity(&self, purpose: KeyPurpose) -> lib_security::Result<KeyHandle> {
        self.trust.device_identity(purpose)
    }
    fn device_credential(&self, out: &mut [u8]) -> lib_security::Result<usize> {
        self.trust.device_credential(out)
    }
}

impl KeyAgreement for CompositeProvider {
    fn custody(&self) -> KeyCustody {
        KeyAgreement::custody(&self.kex)
    }
    fn generate_ephemeral(&mut self, group: KexGroup) -> lib_security::Result<KeyHandle> {
        self.kex.generate_ephemeral(group)
    }
    fn public_share(&self, handle: KeyHandle, out: &mut [u8]) -> lib_security::Result<usize> {
        self.kex.public_share(handle, out)
    }
    fn agree(&mut self, handle: KeyHandle, peer_share: &[u8]) -> lib_security::Result<KeyHandle> {
        self.kex.agree(handle, peer_share)
    }
}

impl KeyDerivation for CompositeProvider {
    fn extract(
        &mut self,
        salt: Option<KeyHandle>,
        ikm: Option<KeyHandle>,
    ) -> lib_security::Result<KeyHandle> {
        self.kdf.extract(salt, ikm)
    }
    fn expand_label(
        &mut self,
        secret: KeyHandle,
        label: &[u8],
        context: &[u8],
        len: usize,
    ) -> lib_security::Result<KeyHandle> {
        self.kdf.expand_label(secret, label, context, len)
    }
    fn expand_to_bytes(
        &mut self,
        secret: KeyHandle,
        label: &[u8],
        context: &[u8],
        out: &mut [u8],
    ) -> lib_security::Result<usize> {
        self.kdf.expand_to_bytes(secret, label, context, out)
    }
}

impl RecordCipher for CompositeProvider {
    fn custody(&self) -> KeyCustody {
        RecordCipher::custody(&self.records)
    }
    fn protection(&self) -> RecordProtection {
        self.records.protection()
    }
    fn seal_record(
        &mut self,
        key: KeyHandle,
        seq: u64,
        aad: &[u8],
        buf: &mut [u8],
        len: usize,
    ) -> lib_security::Result<usize> {
        self.records.seal_record(key, seq, aad, buf, len)
    }
    fn open_record(
        &mut self,
        key: KeyHandle,
        seq: u64,
        aad: &[u8],
        buf: &mut [u8],
        len: usize,
    ) -> lib_security::Result<usize> {
        self.records.open_record(key, seq, aad, buf, len)
    }
}

impl TrustedTime for CompositeProvider {
    fn trust(&self) -> TimeTrust {
        self.clock.trust()
    }
    fn now(&self) -> lib_security::Result<UnixSeconds> {
        self.clock.now()
    }
    fn uptime_millis(&self) -> u64 {
        self.clock.uptime_millis()
    }
}

/// The generic call site `lib-tls` will have: one type parameter, every seam reachable, and the
/// handshake it would drive fails at the first step because nothing is implemented.
fn drive_handshake_prologue<P: TransportProvider>(provider: &mut P) -> lib_security::Result<()> {
    let handle = provider.generate_ephemeral(KexGroup::NistP256)?;
    let mut share = [0u8; 65];
    let _ = provider.public_share(handle, &mut share)?;
    Ok(())
}

#[test]
fn the_five_traits_compose_into_one_type_parameter() {
    let mut provider = CompositeProvider::default();

    // Composition is proven by this compiling: the blanket impl fires only if the concrete type
    // satisfies all five bounds simultaneously.
    assert_eq!(
        drive_handshake_prologue(&mut provider),
        Err(SecurityError::NotImplemented),
        "a composed provider built from placeholders must still refuse every step"
    );

    // Two traits both name a `custody` method; disambiguation is explicit, never ambiguous.
    assert_eq!(TrustStore::custody(&provider), KeyCustody::SoftwareMemory);
    assert_eq!(KeyAgreement::custody(&provider), KeyCustody::SoftwareMemory);
    assert_eq!(RecordCipher::custody(&provider), KeyCustody::SoftwareMemory);
}

// ---- (b) every fail-closed stub returns Err ---------------------------------------------------

/// **The load-bearing test.** Every operation of every placeholder, asserted to refuse.
///
/// A stub that returns success is worse than no stub: it looks implemented, passes a smoke test,
/// and silently authorizes everyone. If a future change makes any of these return `Ok`, this test
/// fails.
#[test]
fn unimplemented_stubs_never_succeed() {
    // --- TrustStore: peer verification and device identity ---
    let trust = UnimplementedTrustStore;
    let mut identity = PeerIdentity::empty();

    let spki_credential = PeerCredential::RawPublicKey { spki: &[0u8; 91] };
    let chain_credential = PeerCredential::X509Chain {
        end_entity: &[0x30, 0x82, 0x01, 0x00],
        intermediates: &[],
    };

    // Both credential forms, and both clock states — including the `Some(now)` case, so a store
    // cannot be "implemented enough" to pass with a trusted clock.
    for credential in [&spki_credential, &chain_credential] {
        for now in [None, Some(UnixSeconds(1_700_000_000))] {
            assert_eq!(
                trust.verify_peer(credential, now, &mut identity),
                Err(SecurityError::NotImplemented),
                "verify_peer must never authorize a peer"
            );
        }
    }
    assert_eq!(
        trust.device_identity(KeyPurpose::Identity),
        Err(SecurityError::NotImplemented)
    );
    assert_eq!(
        trust.device_identity(KeyPurpose::Session),
        Err(SecurityError::NotImplemented)
    );
    let mut credential_out = [0u8; 512];
    assert_eq!(
        trust.device_credential(&mut credential_out),
        Err(SecurityError::NotImplemented)
    );
    // The identity was never populated: an ignored `Err` yields no anchor, no pin, no subject.
    assert_eq!(identity.anchor, AnchorId(0));
    assert_eq!(identity.spki_sha256, [0u8; 32]);
    assert!(identity.subject.is_empty());
    assert_eq!(PEER_SUBJECT_MAX, identity.subject.capacity());

    // --- KeyAgreement: no ephemeral, no share, no shared secret ---
    let mut kex = UnimplementedKeyAgreement;
    for group in [KexGroup::NistP256, KexGroup::NistP384] {
        assert_eq!(
            kex.generate_ephemeral(group),
            Err(SecurityError::NotImplemented),
            "a fabricated handle would flow into the key schedule"
        );
    }
    let mut share = [0u8; 97];
    assert_eq!(
        kex.public_share(KeyHandle(0), &mut share),
        Err(SecurityError::NotImplemented)
    );
    assert_eq!(
        kex.agree(KeyHandle(0), &[0x04; 65]),
        Err(SecurityError::NotImplemented)
    );

    // --- KeyDerivation: no extract, no expand ---
    let mut kdf = UnimplementedKeyDerivation;
    assert_eq!(kdf.extract(None, None), Err(SecurityError::NotImplemented));
    assert_eq!(
        kdf.extract(Some(KeyHandle(0)), Some(KeyHandle(1))),
        Err(SecurityError::NotImplemented)
    );
    assert_eq!(
        kdf.expand_label(KeyHandle(0), b"c ap traffic", b"", 32),
        Err(SecurityError::NotImplemented)
    );
    let mut derived = [0u8; 32];
    assert_eq!(
        kdf.expand_to_bytes(KeyHandle(0), b"exporter", b"", &mut derived),
        Err(SecurityError::NotImplemented)
    );

    // --- RecordCipher: neither direction ---
    let mut records = UnimplementedRecordCipher;
    let mut record = [0u8; 64];
    assert_eq!(
        records.seal_record(KeyHandle(0), 0, b"\x17\x03\x03\x00\x20", &mut record, 32),
        Err(SecurityError::NotImplemented),
        "an Ok here would put plaintext on a socket the caller believes is encrypted"
    );
    assert_eq!(
        records.open_record(KeyHandle(0), 0, b"\x17\x03\x03\x00\x20", &mut record, 48),
        Err(SecurityError::NotImplemented),
        "an Ok here would hand attacker bytes to the broker as verified plaintext"
    );

    // --- TrustedTime: no wall clock ---
    let clock = UnimplementedTime;
    assert_eq!(clock.now(), Err(SecurityError::NotImplemented));
    assert_eq!(clock.trust(), TimeTrust::Unavailable);

    // --- The DER SPKI extractor ---
    let mut spki = [0u8; SPKI_MAX_LEN];
    assert_eq!(
        extract_spki(&[], &mut spki),
        Err(SecurityError::NotImplemented)
    );
    assert_eq!(
        extract_spki(&[0x30, 0x82, 0x01, 0x00], &mut spki),
        Err(SecurityError::NotImplemented)
    );
}

/// The infallible accessors cannot refuse, so they are checked separately: each must report the
/// weakest honest posture, never one a claim or attestation could over-read.
#[test]
fn infallible_accessors_never_overclaim() {
    assert_eq!(UnimplementedTrustStore.model(), TrustModel::Unconfigured);
    assert_eq!(UnimplementedTrustStore.anchor_count(), 0);
    assert_eq!(
        UnimplementedRecordCipher.protection(),
        RecordProtection::NonSecureExported,
        "no record crypto happens anywhere, so no Secure-world boundary may be claimed"
    );
    assert_eq!(UnimplementedTime.trust(), TimeTrust::Unavailable);
    assert_eq!(UnimplementedTime.uptime_millis(), 0);
}

/// `TrustModel::Unconfigured` is not a permissive default. It is documented as "nothing is
/// trusted", and this pins the one policy consequence a caller must implement: a store reporting it
/// can never be used to accept a connection.
#[test]
fn unconfigured_trust_model_is_not_a_working_model() {
    let store = UnimplementedTrustStore;
    assert_ne!(store.model(), TrustModel::PinnedSpki);
    assert_ne!(store.model(), TrustModel::X509Chain);
    assert_ne!(store.model(), TrustModel::RawPublicKey);
    assert_eq!(store.anchor_count(), 0);
}
