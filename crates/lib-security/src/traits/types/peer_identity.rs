use super::anchor_id::AnchorId;

/// Maximum length, in bytes, of the [`PeerIdentity::subject`] label. Bounded on purpose: the
/// subject comes off attacker-controlled DER, so it gets a fixed ceiling rather than an allocation.
/// A longer subject is truncated or refused by the store — never grown.
pub const PEER_SUBJECT_MAX: usize = 64;

/// The resolved identity of a peer whose credential **verified**. This is an *output* type: it is
/// only meaningful after [`verify_peer`](super::trust_store::TrustStore::verify_peer) returned
/// `Ok(())` having written into it. On any error the contents are unspecified and MUST NOT be read
/// — treat a `PeerIdentity` reached through an `Err` path as uninitialized, not as a partial
/// answer.
///
/// [`anchor`](PeerIdentity::anchor) is the field authorization is keyed on (ADR 0013 §6, default
/// deny). The other two fields are for logging and diagnostics; deriving a permission from
/// [`subject`](PeerIdentity::subject) would reintroduce the name-based trust the pinning model
/// deliberately does not use.
///
/// See ADR `docs/decisions/0013-transport-security-trust-store.md` §1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerIdentity {
    /// Which trust-store anchor this peer resolved to — the authorization key.
    pub anchor: AnchorId,
    /// SHA-256 of the peer's DER `SubjectPublicKeyInfo`. Stable across certificate re-issue for the
    /// same key pair, which is what makes it a usable pin.
    pub spki_sha256: [u8; 32],
    /// A human-readable label for logs (e.g. the certificate subject CN), bounded to
    /// [`PEER_SUBJECT_MAX`] bytes. **Never an authorization input** — it is peer-supplied text.
    pub subject: heapless::String<PEER_SUBJECT_MAX>,
}
