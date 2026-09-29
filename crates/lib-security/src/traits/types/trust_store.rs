use super::key_handle::KeyHandle;
use super::peer_credential::PeerCredential;
use super::peer_identity::PeerIdentity;
use super::trust_model::TrustModel;
use super::unix_seconds::UnixSeconds;
use crate::error::Result;
use crate::profile::{KeyCustody, KeyPurpose};

/// **Peer trust and device identity — the single source of both.**
///
/// One trust store, in the Secure world, behind this seam, serving the MQTT/TLS leg and the
/// DDS/TMS identity binding. Not two parallel crypto stacks: the TLS server identity and the
/// `tms.Identity` binding are the *same* handle, in the *same* custody, reported by the *same*
/// [`custody`](TrustStore::custody) accessor. That is the whole point of putting both halves —
/// "who may talk to us" and "who we are" — on one trait.
///
/// # What a real implementation MUST do
///
/// * **Report, never assume.** [`model`](TrustStore::model) tells the caller what a successful
///   [`verify_peer`](TrustStore::verify_peer) actually proved, and [`custody`](TrustStore::custody)
///   tells it where the device's private key really lives. Both are mandatory honesty accessors,
///   the same doctrine as [`KeyStore::custody`](super::key_store::KeyStore::custody): a software
///   store MUST report [`KeyCustody::SoftwareMemory`] so no log, claim or attestation can mistake
///   it for hardware-bound custody.
/// * **Never export the private half.** [`device_identity`](TrustStore::device_identity) returns an
///   opaque [`KeyHandle`], never bytes. There is deliberately no `export_device_private`. When this
///   trait is implemented across the Secure/Non-Secure boundary (the CMSE veneer on a Cortex-M33),
///   **private key material must never leave the Secure world** — the handle crosses, the key does
///   not. [`device_credential`](TrustStore::device_credential) copies out *public* data only.
/// * **Fail closed on every verification.** [`verify_peer`](TrustStore::verify_peer) returns
///   `Result<()>`, not `bool`, so a caller cannot forget to check it. `Ok(())` means *proven*; any
///   `Err` means *deny*. An unresolvable, malformed, oversized or simply unknown credential is an
///   error — never a default anchor, never a "verified but unknown" identity.
/// * **Treat the credential as hostile.** [`PeerCredential`] is attacker-controlled DER. Parsing
///   must be bounded and non-recursive (see [`crate::extract_spki`]); peer-supplied intermediates
///   are never anchors.
/// * **Honor the clock contract.** `now` is `None` when [`TrustedTime`](super::trusted_time::TrustedTime)
///   reports an untrusted clock. A store whose [`model`](TrustStore::model) needs validity windows
///   — [`TrustModel::X509Chain`] — **MUST fail closed on `None`**, because an unchecked expiry is
///   an accepted expired credential. [`TrustModel::PinnedSpki`] has no validity window and so
///   legitimately tolerates `None`; that tolerance is a property of the *model*, not a general
///   permission, and a later switch to chain validation must not inherit it.
///
/// See ADR `docs/decisions/0013-transport-security-trust-store.md` §1, §3 and §5.
pub trait TrustStore {
    /// Which custody tier the device identity key actually has. **Mandatory honesty accessor** —
    /// same doctrine as [`KeyStore::custody`](super::key_store::KeyStore::custody). Never report a
    /// tier stronger than the one physically in force.
    fn custody(&self) -> KeyCustody;

    /// How peers are trusted. **Reported, never assumed** — see [`TrustModel`] for what each model
    /// does and does not prove, and for which models require a trusted clock.
    fn model(&self) -> TrustModel;

    /// How many trust anchors are configured. Zero means nothing can verify; a caller may use this
    /// to refuse to open a listener at all rather than accept connections that must all fail.
    fn anchor_count(&self) -> usize;

    /// Verify a peer credential. On success writes the resolved identity into `out` — the
    /// [`AnchorId`](super::anchor_id::AnchorId) in it is what authorization is keyed on.
    ///
    /// Fails closed with [`crate::SecurityError::UntrustedChain`] when the credential does not
    /// resolve to a configured anchor, [`crate::SecurityError::Malformed`] when it does not parse,
    /// and [`crate::SecurityError::Expired`] when a validity window is enforced and has passed.
    /// `now` is `None` when the clock is untrusted; a store whose [`model`](TrustStore::model)
    /// needs validity windows MUST fail closed on `None`.
    ///
    /// On any error the contents of `out` are unspecified and MUST NOT be read.
    fn verify_peer(
        &self,
        credential: &PeerCredential<'_>,
        now: Option<UnixSeconds>,
        out: &mut PeerIdentity,
    ) -> Result<()>;

    /// The device's own long-lived key for `purpose`. Returns an **opaque handle — never bytes**.
    /// [`KeyPurpose::Identity`] is the one device key: the TLS server identity and the DDS/TMS
    /// identity binding resolve to the same handle.
    fn device_identity(&self, purpose: KeyPurpose) -> Result<KeyHandle>;

    /// Write the device's own certificate (or bare `SubjectPublicKeyInfo`) into `out` as DER, and
    /// return its length. This is **public** data — it is what the device presents to peers, and
    /// what a DDS-Security identity token would carry. An `out` too small fails closed with
    /// [`crate::SecurityError::BufferTooSmall`] — never a truncated or partial credential.
    fn device_credential(&self, out: &mut [u8]) -> Result<usize>;
}
