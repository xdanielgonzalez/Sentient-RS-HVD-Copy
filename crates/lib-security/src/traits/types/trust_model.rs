/// How a [`super::trust_store::TrustStore`] decides that a peer credential is trusted.
///
/// **Reported, never assumed.** A caller reads this to learn what a successful
/// [`verify_peer`](super::trust_store::TrustStore::verify_peer) actually proved — the three models
/// prove materially different things, and a caller that assumed the wrong one would draw a
/// conclusion the store never made. In particular the models differ in whether they need a trusted
/// clock, which is the coupling ADR 0013 §5 calls out explicitly.
///
/// See ADR `docs/decisions/0013-transport-security-trust-store.md` §1 and §3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TrustModel {
    /// No trust model is configured: the store cannot verify anything and every
    /// [`verify_peer`](super::trust_store::TrustStore::verify_peer) call fails closed.
    ///
    /// This is what an unbound or placeholder store reports. A caller MUST treat it as
    /// "nothing is trusted" — it is never a permissive default, and it must never be mapped onto
    /// one of the working models below to make a call site compile.
    Unconfigured,
    /// The peer's `SubjectPublicKeyInfo` is hashed and matched against a short static list of
    /// pinned SPKI hashes. **No chain construction, no `notBefore`/`notAfter`, no CRL, no OCSP.**
    /// This model has no validity window to check, so it is the one model that legitimately
    /// tolerates `now: None` (ADR 0013 §5). Rotation is a signed configuration push, and that is
    /// the *only* revocation mechanism this model has.
    PinnedSpki,
    /// The peer presents an X.509 chain that must build to a configured trust anchor. This model
    /// enforces validity windows and therefore **MUST fail closed when `now` is `None`** — an
    /// absent trusted clock means expiry cannot be checked, and an unchecked expiry is an accepted
    /// expired credential.
    X509Chain,
    /// The peer presents a bare RFC 7250 raw public key, matched against a configured set. Like
    /// [`TrustModel::PinnedSpki`] there is no validity window; unlike it, there is no certificate
    /// wrapper to parse at all.
    RawPublicKey,
}
